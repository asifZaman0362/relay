use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Email {
    pub payload: String,
    pub destination: String,
}
