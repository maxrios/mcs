use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::Instant,
};

const LB_ADDR: &str = "127.0.0.1:64400";
const REDIS_ADDR: &str = "redis://127.0.0.1:6379";
const TEST_DURATION: Duration = Duration::from_secs(5);
const MB: usize = 1024 * 1024;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    match mode {
        "conn" => test_connection_limit().await,
        "bandwidth" => test_bandwidth_limit().await,
        "sink" => run_sink_server().await,
        _ => {
            println!("Usage: cargo run --bin stress_test -- [conn|bandwidth]");
            println!(
                "  connection : Attempts to open thousands of connections to trigger connection limiting."
            );
            println!(
                "  bandwidth  : Establishes one connection and blasts data to trigger throughput limiting."
            );
        }
    }
}

async fn test_connection_limit() {
    println!("--- Starting Connection Flood Test ---");
    println!("Target: {}", LB_ADDR);

    let success = Arc::new(AtomicUsize::new(0));
    let failures = Arc::new(AtomicUsize::new(0));
    let start = Instant::now();
    let mut handles = vec![];

    for _ in 0..100 {
        let succ = success.clone();
        let fail = failures.clone();

        handles.push(tokio::spawn(async move {
            let deadline = Instant::now() + TEST_DURATION;

            while Instant::now() < deadline {
                match TcpStream::connect(LB_ADDR).await {
                    Ok(mut stream) => {
                        let mut buf = [0u8; 1];

                        let sleep = tokio::time::sleep(Duration::from_millis(50));

                        tokio::select! {
                            _ = stream.read(&mut buf) => {
                                fail.fetch_add(1, Ordering::Relaxed);
                            }
                            _ = sleep => {
                                succ.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                    Err(_) => {
                        fail.fetch_add(1, Ordering::Relaxed);
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }));
    }

    for h in handles {
        let _ = h.await;
    }

    let s = success.load(Ordering::Relaxed);
    let f = failures.load(Ordering::Relaxed);

    println!("Done.");
    println!("Successesful connections: {}", s);
    println!("Blocked connections: {}", f);
    println!(
        "Rate: {:.2} conn/sec",
        (s + f) as f64 / start.elapsed().as_secs_f64()
    );

    if f > 0 {
        println!("PASS: Some connections were rejected.");
    } else {
        println!("FAIL: No connections were rejected. Limits might be too high.");
    }
}

/// This test requires disabling TLS termination on the load balancer.
async fn test_bandwidth_limit() {
    println!("--- Starting Bandwidth Test ---");

    let payload_size = 2 * MB;
    let payload = vec![0u8; payload_size];
    let mut stream = TcpStream::connect(LB_ADDR)
        .await
        .expect("failed to connect to test load balancer");

    println!("Connected. Attempting to send {} MB...", payload_size / MB);

    let start = Instant::now();
    if let Err(e) = stream.write_all(&payload).await {
        println!("Write failed: {}", e);
        println!("Disable TLS on the load balancer.");
        return;
    }

    let duration = start.elapsed();
    let seconds = duration.as_secs_f64();
    let mb_per_sec = (payload_size / MB) as f64 / seconds;

    println!("Transfer complete.");
    println!("Time: {:.2}s", seconds);
    println!("Speed: {:.2} MB/s", mb_per_sec);

    println!("Compare transfer speed against the configured quota.");
}

async fn run_sink_server() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind sink");
    let port = listener.local_addr().unwrap().port();
    let ip = listener.local_addr().unwrap().ip();
    let addr = format!("{ip}:{port}");

    println!("sink server running on {}", addr);

    let client = redis::Client::open(REDIS_ADDR).expect("invalid redis address");
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .expect("redis failed to connect");

    let redis_key = format!("mcs:node:{addr}");
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            interval.tick().await;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let _: () = redis::cmd("ZADD")
                .arg("mcs:node")
                .arg(now)
                .arg(&addr)
                .query_async(&mut conn)
                .await
                .unwrap_or_else(|e| eprintln!("Redis heartbeat failed: {}", e));
        }
    });

    println!(
        "Registered '{}' in Redis. The load balancer should pick it up in ~2s.",
        redis_key
    );
    println!("   (Press Ctrl+C to stop)");

    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        tokio::spawn(async move {
            let mut buf = [0u8; 1024 * 64];
            loop {
                match socket.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        });
    }
}
