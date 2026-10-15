#![windows_subsystem = "windows"]

use rust_app::server::{self, AppState};
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tao::dpi::LogicalSize;
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, Mutex};
use wry::WebViewBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    rust_app::updater::cleanup_old_executable();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let (tx_stream, _) = broadcast::channel::<String>(4096);

    let state = AppState {
        is_running: Arc::new(AtomicBool::new(false)),
        tx_stream,
        results_store: Arc::new(Mutex::new(HashMap::new())),
        all_input_ips: Arc::new(Mutex::new(Vec::new())),
    };

    let (port_tx, port_rx) = std::sync::mpsc::channel::<u16>();

    let server_state = state.clone();
    rt.spawn(async move {
        let listener = match TcpListener::bind("127.0.0.1:45678").await {
            Ok(l) => l,
            Err(_) => TcpListener::bind("127.0.0.1:0").await.expect("Failed to bind port"),
        };
        let local_addr = listener.local_addr().expect("Failed to get local address");
        port_tx.send(local_addr.port()).expect("Failed to send port");

        let app = server::create_router(server_state);
        axum::serve(listener, app).await.expect("Server failed");
    });

    let port = port_rx.recv().expect("Failed to receive port");
    let server_url = format!("http://127.0.0.1:{}", port);

    let event_loop = EventLoopBuilder::new().build();
    let window = WindowBuilder::new()
        .with_title("Teltonika Orchestrator v1.2.0")
        .with_inner_size(LogicalSize::new(1380.0, 860.0))
        .with_min_inner_size(LogicalSize::new(1024.0, 700.0))
        .build(&event_loop)?;

    let _webview = WebViewBuilder::new()
        .with_url(&server_url)
        .build(&window)?;

    event_loop.run(move |event, _, control_flow| {
        let _ = &_webview;
        *control_flow = ControlFlow::Wait;
        if let tao::event::Event::WindowEvent {
            event: tao::event::WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}
