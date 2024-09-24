use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use axum::{
    extract::{FromRef, State},
    http::header,
    response::{sse::Event, IntoResponse, Sse},
    routing::{get, post},
    Json, Router,
};
use axum_embed::ServeEmbed;
use counter::Counter;
use include_ext::IncludeExt;
use rand::Rng;
use rust_embed::RustEmbed;
use serde_json::json;
use stop_signal::StopSignal;
use tokio::{net::TcpListener, signal};
use tower_http::{cors::CorsLayer, timeout::TimeoutLayer, trace::TraceLayer};

use futures::stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as _;

use rinja_axum::Template;

use local_ip_address::local_ip;
use qrcode_generator::QrCodeEcc;

mod counter;
mod include_ext;
mod stop_signal;

const IP: IpAddr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
const PORT: u16 = 8080;

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

#[derive(Clone, Default, FromRef)]
struct AppState {
    counter: Counter,
    stop_signal: StopSignal,
}

#[tokio::main]
async fn main() {
    let stop_signal = StopSignal::default();

    let app = Router::new()
        .route(
            "/",
            get(|State(counter): State<Counter>| async move {
                IndexTemplate {
                    counter: counter.value(),
                }
            }),
        )
        .route(
            "/qrcode",
            get(|| async move {
                let local_ip = local_ip().unwrap();
                let data = qrcode_generator::to_png_to_vec(
                    format!("http://{local_ip}:{PORT}"),
                    QrCodeEcc::Low,
                    300,
                )
                .unwrap();

                let mime = mime_guess::mime::PNG;

                ([(header::CONTENT_TYPE, mime.as_ref())], data).into_response()
            }),
        )
        .nest(
            "/counter",
            Router::new()
                .route(
                    "/increment",
                    post(|State(counter): State<Counter>| async move {
                        if rand::thread_rng().gen_range(0..10) > 7 {
                            tokio::time::sleep(Duration::from_secs_f32(0.5)).await;
                        }
                        counter.increment()
                    }),
                )
                .route(
                    "/decrement",
                    post(|State(counter): State<Counter>| async move {
                        if rand::thread_rng().gen_range(0..10) > 7 {
                            tokio::time::sleep(Duration::from_secs_f32(0.5)).await;
                        }
                        counter.decrement()
                    }),
                )
                .route(
                    "/",
                    get(|State(counter): State<Counter>| async move {
                        Json(json!({"counter": counter.value()}))
                    }),
                )
                .route(
                    "/sse",
                    get(|State(app_state): State<AppState>| async move {
                        let counter = app_state.counter;
                        let stop_signal = app_state.stop_signal;

                        let stream = BroadcastStream::new(counter.subscribe()).map(|i| {
                            let counter = i.unwrap();
                            Event::default()
                                .event("update")
                                .json_data(json!({"counter": counter}))
                        });

                        let first = stream::once(async move {
                            let counter = counter.value();
                            Event::default()
                                .event("update")
                                .json_data(json!({"counter": counter}))
                        });

                        let stop = stream::once(async move {
                            let _ = stop_signal.subscribe().recv().await;
                            Ok(Event::default().event("close").data("Stream closed!"))
                        });

                        Sse::new(first.chain(stop.merge(stream))).keep_alive(
                            axum::response::sse::KeepAlive::new().text("keep-alive-text"),
                        )
                    }),
                ),
        )
        .fallback_service(ServeEmbed::<Assets>::new())
        .with_state(AppState {
            stop_signal: stop_signal.clone(),
            ..Default::default()
        })
        .layer((
            TraceLayer::new_for_http(),
            TimeoutLayer::new(Duration::from_secs(10)),
            CorsLayer::very_permissive(),
        ));

    #[cfg(debug_assertions)]
    let app = app.layer(tower_livereload::LiveReloadLayer::new());

    let listener = TcpListener::bind(SocketAddr::new(IP, PORT)).await.unwrap();

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(stop_signal))
        .await
        .unwrap();
}

async fn shutdown_signal(stop_signal: StopSignal) {
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

    stop_signal.send_stop()
}
