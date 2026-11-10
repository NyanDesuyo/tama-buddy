use crate::{
    api::{CareRequest, ErrorResponse, HatchRequest, Snapshot, StatusSummary},
    now,
    store::Store,
};
use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use std::{
    net::SocketAddr,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::watch;

#[derive(Clone)]
struct SharedStore {
    store: Arc<Mutex<Store>>,
    dialogue: crate::ollama::Dialogue,
    summary_gate: Arc<tokio::sync::Semaphore>,
}
struct ApiError(StatusCode, String);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(ErrorResponse { error: self.1 })).into_response()
    }
}
impl ApiError {
    fn internal(error: impl std::fmt::Display) -> Self {
        eprintln!("Backend error: {error}");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not update the pet save.".into(),
        )
    }
    fn invalid(error: JsonRejection) -> Self {
        Self(error.status(), error.body_text())
    }
}

fn snapshot(
    store: &Store,
    result: (Option<crate::pet::Pet>, Option<String>),
    time: i64,
) -> Result<Snapshot> {
    Ok(Snapshot {
        progression: result.0.as_ref().map(|p| p.progression(time)),
        pet: result.0,
        message: result.1,
        events: store.events()?,
        server_time: time,
    })
}

// rusqlite is synchronous. Run database work on blocking workers instead of
// blocking Tokio's network executor; the mutex serializes requests and ticks.
async fn with_store<T: Send + 'static>(
    state: SharedStore,
    operation: impl FnOnce(&mut Store) -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(move || {
        let mut store = state.store.lock().map_err(ApiError::internal)?;
        operation(&mut store)
    })
    .await
    .map_err(ApiError::internal)?
}

async fn status(State(state): State<SharedStore>) -> Result<Json<Snapshot>, ApiError> {
    with_store(state.clone(), |store| {
        let time = now();
        let result = store.refresh(time).map_err(ApiError::internal)?;
        snapshot(store, result, time).map_err(ApiError::internal)
    })
    .await
    .map(Json)
}
async fn hatch(
    State(state): State<SharedStore>,
    body: Result<Json<HatchRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<Snapshot>), ApiError> {
    let Json(body) = body.map_err(ApiError::invalid)?;
    let name = body.name.trim();
    if name.chars().count() > 20 || name.chars().any(char::is_control) {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "Use a name with at most 20 characters and no control characters.".into(),
        ));
    }
    let name = if name.is_empty() { "Mochi" } else { name }.to_owned();
    with_store(state.clone(), move |store| {
        let time = now();
        if store
            .refresh(time)
            .map_err(ApiError::internal)?
            .0
            .is_some_and(|p| p.alive())
        {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "A buddy is already living here.".into(),
            ));
        }
        let result = store.hatch(name, time).map_err(ApiError::internal)?;
        snapshot(store, result, time).map_err(ApiError::internal)
    })
    .await
    .map(|snapshot| (StatusCode::CREATED, Json(snapshot)))
}
async fn care(
    State(state): State<SharedStore>,
    body: Result<Json<CareRequest>, JsonRejection>,
) -> Result<Json<Snapshot>, ApiError> {
    let Json(body) = body.map_err(ApiError::invalid)?;
    with_store(state.clone(), move |store| {
        let time = now();
        let result = store.care(body.action, time).map_err(ApiError::internal)?;
        if result.0.is_none() {
            return Err(ApiError(
                StatusCode::NOT_FOUND,
                "Hatch a buddy first.".into(),
            ));
        }
        snapshot(store, result, time).map_err(ApiError::internal)
    })
    .await
    .map(Json)
}

async fn summarize(State(state): State<SharedStore>) -> Result<Json<StatusSummary>, ApiError> {
    let _permit = state.summary_gate.try_acquire().map_err(|_| {
        ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Your buddy is already thinking. Try again shortly.".into(),
        )
    })?;
    let (pet, time) = with_store(state.clone(), |store| {
        let time = now();
        let pet = store
            .refresh(time)
            .map_err(ApiError::internal)?
            .0
            .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "Hatch a buddy first.".into()))?;
        if !pet.alive() {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "Your buddy has passed away. Hatch a new egg first.".into(),
            ));
        }
        Ok((pet, time))
    })
    .await?;
    // Release the SQLite lock before waiting for the model. Care stays available.
    state.dialogue.summarize(&pet, time).await.map(Json).map_err(|error| {
        eprintln!("Ollama summary failed: {error:#}");
        ApiError(StatusCode::BAD_GATEWAY,
            "Could not get a response from Ollama. Check that Ollama is running and the configured model is installed.".into())
    })
}
fn router(state: SharedStore) -> Router {
    Router::new()
        .route("/", get(crate::web::index))
        .route("/assets/app.css", get(crate::web::stylesheet))
        .route("/assets/app.js", get(crate::web::javascript))
        .route("/favicon.svg", get(crate::web::favicon))
        .route("/health", get(|| async { Json("ok") }))
        .route("/api/pet", get(status).post(hatch))
        .route("/api/pet/actions", axum::routing::post(care))
        .route("/api/pet/summary", axum::routing::post(summarize))
        .fallback(|| async { ApiError(StatusCode::NOT_FOUND, "Unknown API route.".into()) })
        .layer(DefaultBodyLimit::max(4096))
        .with_state(state)
}

#[tokio::main]
pub async fn run(path: &Path, address: SocketAddr) -> Result<()> {
    let state = SharedStore {
        store: Arc::new(Mutex::new(Store::open(path)?)),
        dialogue: crate::ollama::Dialogue::from_env()?,
        summary_gate: Arc::new(tokio::sync::Semaphore::new(1)),
    };
    // Catch up once before serving requests, then keep advancing without a TUI.
    with_store(state.clone(), |s| {
        s.refresh(now()).map_err(ApiError::internal)
    })
    .await
    .map_err(|e| anyhow::anyhow!(e.1))?;
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .context("Could not bind backend address")?;
    let address = listener.local_addr()?;
    let (stop, mut stopping) = watch::channel(false);
    let timer_state = state.clone();
    let simulation = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let _ = with_store(timer_state.clone(), |s| s.refresh(now()).map_err(ApiError::internal)).await;
                }
                _ = stopping.changed() => break,
            }
        }
    });
    println!(
        "Tama Buddy backend listening on http://{address}\nSQLite save: {}",
        path.display()
    );
    let result = axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await;
    let _ = stop.send(true);
    simulation.await?;
    result?;
    println!("Backend stopped. All changes saved.");
    Ok(())
}
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Could not register SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
