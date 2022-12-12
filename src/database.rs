use actix::Addr;

use crate::types::Email;
use rusqlite::Connection;
use std::collections::HashMap;

#[allow(unused)]
#[derive(Debug)]
pub enum DatabaseError {
    Invalid,
    NoRows,
}

pub struct Session {
    pub addr: Option<Addr<crate::Client>>,
    pub alive: bool,
    pub password: Option<String>,
}

pub struct Database {
    sessions: HashMap<String, Session>,
    connection: Connection,
}

impl Database {
    pub fn new(db_path: String) -> Database {
        Database {
            sessions: HashMap::<String, Session>::new(),
            connection: Connection::open(db_path).unwrap(),
        }
    }

    pub fn get_addr(&self, identifier: String) -> Option<Addr<crate::Client>> {
        match self.sessions.get(&identifier) {
            Some(session) => session.addr.clone(),
            None => None,
        }
    }

    #[allow(unused)]
    pub fn get_mails_for(&self, identifier: &str) -> Result<Vec<Email>, rusqlite::Error> {
        let mut result = vec![];
        /*let mut sql = self
            .connection
            .prepare("SELECT rowid, * FROM MAIL WHERE dest = ?")
            .unwrap();
        let rows = sql.query_map([identifier], |row| Ok(row.get(0)?))?;
        for row in rows {
            let id: i64 = row?;
            let mut blob = self.connection.blob_open(
                rusqlite::DatabaseName::Main,
                "mail",
                "payload",
                id,
                true,
            )?;
            let mut serialized = vec![];
            blob.read_to_end(&mut serialized).unwrap();
            let deserialized: Email = bincode::deserialize(&serialized).unwrap();
            result.push(deserialized);
        }*/
        let mut sql = self
            .connection
            .prepare("SELECT * FROM MAIL WHERE dest = ?")
            .unwrap();
        let rows = sql.query_map([identifier], |row| {
            Ok(Email {
                destination: row.get(0)?,
                payload: row.get(1)?,
            })
        })?;
        for row in rows {
            match row {
                Ok(mail) => result.push(mail),
                Err(err) => {}
            };
        }
        Ok(result)
    }

    pub fn save_mail_for(
        &self,
        identifier: String,
        mail: String,
    ) -> Result<usize, rusqlite::Error> {
        /*let serialized = bincode::serialize(&mail).unwrap();
        self.connection.execute(
            "INSERT INTO MAIL (dest, payload) VALUES (?1, ?2)",
            (&identifier, ZeroBlob(serialized.len() as i32)),
        )?;
        let rowid = self.connection.last_insert_rowid();
        let mut blob = self.connection.blob_open(
            rusqlite::DatabaseName::Main,
            "mail",
            "payload",
            rowid,
            false,
        )?;
        let written = blob.write(&serialized).unwrap();
        assert_eq!(written, serialized.len());
        Ok(serialized.len())*/
        self.connection.execute(
            "INSERT INTO MAIL (dest, payload) VALUES (?1, ?2)",
            (&identifier, &mail),
        )
    }

    pub fn activate_self(
        &mut self,
        identifier: String,
        password: String,
        addr: Addr<crate::Client>,
    ) -> Result<(), DatabaseError> {
        match self.sessions.get_mut(&identifier) {
            Some(session) => {
                session.alive = true;
                session.password = Some(password);
                session.addr = Some(addr.clone());
                Ok(())
            }
            None => {
                self.sessions.insert(
                    identifier.clone(),
                    Session {
                        addr: Some(addr.clone()),
                        alive: true,
                        password: Some(password),
                    },
                );
                Ok(())
            }
        }
    }

    pub fn deactivate(&mut self, id: String) {
        match self.sessions.get_mut(&id) {
            Some(session) => {
                session.alive = false;
                session.password = None
            }
            None => {}
        }
    }

    pub fn is_alive(&self, identifier: &String) -> Result<bool, DatabaseError> {
        match self.sessions.get(identifier) {
            Some(session) => Ok(session.alive),
            None => Err(DatabaseError::NoRows),
        }
    }
}
