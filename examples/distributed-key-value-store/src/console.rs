use std::str::from_utf8;

use anyhow::anyhow;
use tokio::io::{stdin, AsyncBufReadExt, BufReader};

use super::rpc::KVServiceClientImpl;

async fn handle_input_line(kv_client: &mut KVServiceClientImpl, line: &str) {
    let mut args = line.split(' ');
    match args.next() {
        Some("GET") => {
            let key = {
                match args.next() {
                    Some(key) => key,
                    None => {
                        eprintln!("Expected key");
                        return;
                    }
                }
            };
            match kv_client.get(key.as_bytes()).await {
                Ok(value) => println!("Got {}: {}", key, from_utf8(&value).unwrap()),
                Err(err) => {
                    eprintln!("Got {} error: {}", key, err)
                }
            }
        }
        Some("GET_PROVIDERS") => {
            let key = {
                match args.next() {
                    Some(key) => {}
                    None => {
                        eprintln!("Expected key");
                        return;
                    }
                }
            };
        }
        Some("PUT") => {
            let key = {
                match args.next() {
                    Some(key) => key,
                    None => {
                        eprintln!("Expected key");
                        return;
                    }
                }
            };
            let value = {
                match args.next() {
                    Some(value) => value.as_bytes(),
                    None => {
                        eprintln!("Expected value");
                        return;
                    }
                }
            };

            match kv_client.put(key.as_bytes(), value).await {
                Ok(_) => {}
                Err(err) => eprintln!("Put {} error: {}", key, err),
            }
        }
        Some("PUT_PROVIDER") => {
            let key = {
                match args.next() {
                    Some(key) => {}
                    None => {
                        eprintln!("Expected key");
                        return;
                    }
                }
            };
        }
        _ => {
            eprintln!("expected GET, GET_PROVIDERS, PUT or PUT_PROVIDER");
        }
    }
}

pub(crate) async fn run(addr: &str) -> anyhow::Result<()> {
    let mut kv_client = KVServiceClientImpl::new(&addr)
        .await
        .map_err(|err| anyhow!("connect {} error: {}", addr, err))?;

    let mut stdin = BufReader::new(stdin()).lines();
    loop {
        if let Some(line) = stdin.next_line().await? {
            handle_input_line(&mut kv_client, &line).await
        }
    }
    Ok(())
}
