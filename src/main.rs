use prometheus::{register_gauge, Encoder, Gauge, TextEncoder};
use serialport;
use std::io::{self, BufRead};
use std::sync::Arc;
use std::time::Duration;
use hyper::{Body, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};
use tokio::task::spawn;
use tokio::signal;
use tokio::sync::oneshot;
use std::convert::Infallible;

async fn metrics_handler(_: Request<Body>) -> Result<Response<Body>, Infallible> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    let response = Response::builder()
        .status(200)
        .header("Content-Type", encoder.format_type())
        .body(Body::from(buffer))
        .unwrap();
    Ok(response)
}

async fn start_http_server(
    g: Arc<Gauge>,
    shutdown_rx: oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = ([0, 0, 0, 0], 9898).into();

    // Create the make_service using make_service_fn
    let make_svc = make_service_fn(|_conn| {
        let g = g.clone();
        async move {
            Ok::<_, Infallible>(service_fn(metrics_handler))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);

    println!("Serving metrics on http://{}", addr);

    // Create a future that resolves when the shutdown signal is received
    let graceful = server.with_graceful_shutdown(async {
        shutdown_rx.await.ok();
    });

    // Await the server's completion
    if let Err(e) = graceful.await {
        eprintln!("Server error: {}", e);
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let port_name = "/dev/ttyACM0";
    let baud_rate = 9_600;

    // Create a channel to signal shutdown
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    // Listen for Ctrl+C or another signal to gracefully shutdown
    let signal_handle = tokio::spawn(async move {
        signal::ctrl_c()
            .await
            .expect("Failed to listen for shutdown signal");
        let _ = shutdown_tx.send(());
    });

    // Try to open the serial port
    let port = serialport::new(port_name, baud_rate)
        .timeout(Duration::from_millis(2000))
        .open();

    // Register a Prometheus gauge metric
    let moisture_gauge = register_gauge!("moisture", "Soil Moisture").unwrap();
    let moisture_gauge = Arc::new(moisture_gauge);

    // Clone the gauge for the serial task
    let moisture_gauge_serial = Arc::clone(&moisture_gauge);

    // Start the HTTP server in a new async task with shutdown support
    let server_handle = spawn(async move {
        start_http_server(moisture_gauge.clone(), shutdown_rx)
            .await
            .unwrap();
    });

    match port {
        Ok(mut port) => {
            println!(
                "Receiving data on {} at {} baud:",
                &port_name, &baud_rate
            );

            // Move serial reading into a blocking thread
            tokio::task::spawn_blocking(move || {
                let mut reader = io::BufReader::new(port);
                loop {
                    let mut serial_buf = String::new();
                    match reader.read_line(&mut serial_buf) {
                        Ok(_t) => {
                            if let Ok(m) = serial_buf.trim().parse::<f64>() {
                                moisture_gauge_serial.set(m);
                            }
                        }
                        Err(e) => eprintln!("{}", e),
                    }
                }
            });
        }
        Err(e) => {
            eprintln!("Failed to open {}. Error: {}", port_name, e);
        }
    }

    // Wait for the server and signal to complete
    tokio::select! {
        _ = server_handle => {},
        _ = signal_handle => {
            println!("Shutdown signal received");
        }
    }

    Ok(())
}
