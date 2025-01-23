use configuration::CONFIGURATION;
use log::{error, info, LevelFilter};
use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectOptions, Database, DbErr};
use std::time::Duration;

mod api;
mod configuration;
mod database;
mod encryption;
mod error;
mod file;
mod hash;
mod request;
mod util;

#[tokio::main]
async fn main() -> Result<(), DbErr> {
    env_logger::init();

    /* Init configuration */
    let connection_string = &CONFIGURATION.connection_string;

    info!("Connecting to database...");

    let mut connect_options = ConnectOptions::new(connection_string);

    connect_options
        .sqlx_logging_level(LevelFilter::Debug)
        .max_connections(5)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(8))
        .acquire_timeout(Duration::from_secs(8))
        .idle_timeout(Duration::from_secs(8))
        .max_lifetime(Duration::from_secs(8));

    let database_connection = Database::connect(connect_options).await?;

    info!("Migrating database...");
    Migrator::up(&database_connection, None).await?;

    info!("Starting API on {}...", &CONFIGURATION.listening_address);

    if let Err(error) = api::listen(database_connection.clone()).await {
        error!("API failed: {error}");
    }

    info!("API shut down. Closing database connection...");
    database_connection.close().await?;

    info!("Bye.");
    Ok(())
}