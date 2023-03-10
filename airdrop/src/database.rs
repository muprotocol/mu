use std::sync::{Arc, Mutex};

use log::error;
use rusqlite::Connection;
use solana_sdk::pubkey::Pubkey;

use crate::Error;

const DATABASE_FILE: &str = "./database.sqlite";

pub struct Database {
    connection: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn open() -> Result<Self, Error> {
        let connection = Connection::open(DATABASE_FILE).map_err(|e| {
            error!("Can not open database: {e:?}");
            Error::FailedToProcessTransaction
        })?;

        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS users (
                    email   TEXT UNIQUE NOT NULL,
                    account TEXT NOT NULL
                )",
                (), // empty list of parameters.
            )
            .map_err(|e| {
                error!("Can not initialize database: {e:?}");
                Error::FailedToProcessTransaction
            })?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn insert_user(&self, email: &str, pubkey: &Pubkey) -> Result<(), Error> {
        self.connection
            .lock()
            .map_err(|e| {
                error!("Can not lock database mutex: {e:?}");
                Error::FailedToProcessTransaction
            })?
            .execute(
                "INSERT OR IGNORE INTO users(email, account)
                 VALUES (?1, ?2)",
                (&email, &pubkey.to_string()),
            )
            .map_err(|e| {
                error!("Can not insert user into database: {e:?}");
                Error::FailedToProcessTransaction
            })?;
        Ok(())
    }
}
