use std::{
    borrow::Cow,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    extract::{FromRef, State},
    response::{sse::Event, Sse},
    routing::{get, post},
    Json, Router,
};
use axum_embed::ServeEmbed;
use rust_embed::RustEmbed;
use serde_json::json;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::{net::TcpListener, signal};
use tower_http::{cors::CorsLayer, timeout::TimeoutLayer, trace::TraceLayer};

use futures::stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as _;

use rinja_axum::Template;

trait IncludeExt<T>
where
    T: RustEmbed,
{
    fn include(&self, file_path: &'static str) -> Cow<'static, str> {
        match T::get(file_path).map(|f| f.data) {
            Some(Cow::Borrowed(bytes)) => {
                // Преобразуем байты в строку
                core::str::from_utf8(bytes)
                    .map(Cow::Borrowed)
                    .unwrap_or_default()
            }
            Some(Cow::Owned(bytes)) => {
                // Преобразуем байты в строку и создаем Cow::Owned
                String::from_utf8(bytes).map(Cow::Owned).unwrap_or_default()
            }
            None => "".into(),
        }
    }
}

#[derive(RustEmbed, Clone)]
#[folder = "templates"]
struct Templates;

#[derive(Template)]
#[template(path = "index.jinja", ext = "html")]
struct IndexTemplate {
    counter: i32,
}

impl IncludeExt<Templates> for IndexTemplate {}

#[derive(RustEmbed, Clone)]
#[folder = "assets"]
struct Assets;

#[derive(Clone, Debug)]
struct Counter {
    value: Arc<Mutex<i32>>,
    sender: Sender<i32>,
}

impl Default for Counter {
    fn default() -> Self {
        Self {
            value: Default::default(),
            sender: Sender::new(2),
        }
    }
}

impl Counter {
    fn value(&self) -> i32 {
        *self.value.lock().unwrap()
    }

    fn increment(&self) {
        let value = {
            let mut counter = self.value.lock().unwrap();
            *counter += 1;
            *counter
        };

        let _ = self.sender.send(value);
    }

    fn decrement(&self) {
        let value = {
            let mut counter = self.value.lock().unwrap();
            *counter -= 1;
            *counter
        };
        let _ = self.sender.send(value);
    }

    fn subscribe(&self) -> Receiver<i32> {
        self.sender.subscribe()
    }
}

#[derive(Clone, Default, FromRef)]
struct AppState {
    counter: Counter,
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route(
            "/",
            get(|State(counter): State<Counter>| async move {
                IndexTemplate {
                    counter: counter.value(),
                }
            }),
        )
        .nest(
            "/counter",
            Router::new()
                .route(
                    "/increment",
                    post(|State(counter): State<Counter>| async move { counter.increment() }),
                )
                .route(
                    "/decrement",
                    post(|State(counter): State<Counter>| async move { counter.decrement() }),
                )
                .route(
                    "/",
                    get(|State(counter): State<Counter>| async move {
                        Json(json!({"counter": counter.value()}))
                    }),
                )
                .route(
                    "/sse",
                    get(|State(counter): State<Counter>| async move {
                        let stream = BroadcastStream::new(counter.subscribe()).map(|i| {
                            let counter = i.unwrap();
                            Event::default().json_data(json!({"counter": counter}))
                        });

                        let first = stream::once(async move {
                            let counter = counter.value();
                            Event::default().json_data(json!({"counter": counter}))
                        });

                        Sse::new(first.chain(stream)).keep_alive(
                            axum::response::sse::KeepAlive::new().text("keep-alive-text"),
                        )
                    }),
                ),
        )
        .fallback_service(ServeEmbed::<Assets>::new())
        .with_state(AppState::default())
        .layer((
            TraceLayer::new_for_http(),
            TimeoutLayer::new(Duration::from_secs(10)),
            CorsLayer::very_permissive(),
        ));

    let listener = TcpListener::bind("0.0.0.0:8080").await.unwrap();

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
