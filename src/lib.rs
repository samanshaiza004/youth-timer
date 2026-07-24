pub mod model;

#[cfg(all(target_os = "wasi", target_env = "p2"))]
mod app;

#[cfg(all(target_os = "wasi", target_env = "p2"))]
youth_sdk::export_app!(app::Timer);
