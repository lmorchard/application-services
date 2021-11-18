use futures_util::TryStreamExt;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use std::thread;
use tokio::task;

use env_logger::Env;
use nimbus::{
    error::Result, AppContext, AvailableRandomizationUnits, EnrollmentStatus, NimbusClient,
    RemoteSettingsConfig,
};

const DEFAULT_BASE_URL: &str = "https://firefox.settings.services.mozilla.com";
const DEFAULT_COLLECTION_NAME: &str = "messaging-experiments";

async fn api_index() -> hyper::Result<Response<Body>> {
    Ok(Response::new(Body::from(
        "Try POSTing data to /echo such as: `curl localhost:3000/echo -XPOST -d 'hello world'`",
    )))
}

async fn api_experiments() -> hyper::Result<Response<Body>> {
    let result = || {
        let config = r#"{
            "context": {
                "app_id": "org.mozilla.fenix",
                "app_name": "fenix",
                "channel": "release",
                "locale": "en-US",
                "installation_date": 1628596800000
            },
            "collection_name": "nimbus-mobile-experiments",
            "client_id": "{ffffffff-ffff-463e-9462-3df844e19204}"
        }"#;

        let config = serde_json::from_str::<serde_json::Value>(&config)?;

        let server_url = DEFAULT_BASE_URL;

        let context = config.get("context").unwrap();
        let context = serde_json::from_value::<AppContext>(context.clone())?;

        let client_id = config
            .get("client_id")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "no-client-id-specified".to_string());
        log::info!("Client ID is {}", client_id);

        let collection_name = match config.get("collection_name") {
            Some(v) => v.as_str().unwrap(),
            _ => DEFAULT_COLLECTION_NAME,
        };
        log::info!("Collection name is {}", collection_name);

        // let temp_dir = std::env::temp_dir();
        // let db_path_default = temp_dir.to_str().unwrap();
        let db_path = "./web-user"; // TODO derive from request?

        let remote_settings_config = RemoteSettingsConfig {
            server_url: server_url.to_string(),
            collection_name: collection_name.to_string(),
        };

        // HACK: Need to fire up a thread for fetch_experiments because of conflicting async runtimes
        let nimbus_client = NimbusClient::new(
            context.clone(),
            db_path,
            Some(remote_settings_config.clone()),
            AvailableRandomizationUnits::with_client_id(&client_id),
        )?;
        thread::spawn(move || nimbus_client.fetch_experiments())
            .join()
            .expect("Fetch experiments thread panicked")?;

        let nimbus_client = NimbusClient::new(
            context.clone(),
            db_path,
            Some(remote_settings_config.clone()),
            AvailableRandomizationUnits::with_client_id(&client_id),
        )?;
        log::info!("Nimbus ID is {}", nimbus_client.nimbus_id()?);
        nimbus_client.apply_pending_experiments()?;
        // nimbus_client.get_all_experiments()
        nimbus_client.get_active_experiments()
    };

    let res = match result() {
        Ok(experiments) => Response::builder()
            .status(StatusCode::OK)
            .body(format!("{:?}", experiments).into())
            .unwrap(),
        Err(error) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(format!("Error: {:?}", error).into())
            .unwrap(),
    };

    Ok(res)
}

/// This is our service handler. It receives a Request, routes on its
/// path, and returns a Future of a Response.
async fn service_main(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => api_index().await,
        (&Method::POST, "/experiments") => api_experiments().await,
        _ => {
            let mut not_found = Response::default();
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::from_env(Env::default().default_filter_or("info")).init();
    viaduct_reqwest::use_reqwest_backend();

    let addr = ([127, 0, 0, 1], 8675).into();
    let service = make_service_fn(|_| async { Ok::<_, hyper::Error>(service_fn(service_main)) });
    let server = Server::bind(&addr).serve(service);

    println!("Listening on http://{}", addr);

    server.await?;

    Ok(())
}
