use db::kvp::KeyValueStore;
use util::ResultExt;

pub struct Session {
    session_id: String,
    old_session_id: Option<String>,
}

const SESSION_ID_KEY: &str = "session_id";

impl Session {
    pub async fn new(session_id: String, db: KeyValueStore) -> Self {
        let old_session_id = db.read_kvp(SESSION_ID_KEY).ok().flatten();

        db.write_kvp(SESSION_ID_KEY.to_string(), session_id.clone())
            .await
            .log_err();

        Self {
            session_id,
            old_session_id,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test() -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            old_session_id: None,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test_with_old_session(old_session_id: String) -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            old_session_id: Some(old_session_id),
        }
    }

    pub fn id(&self) -> &str {
        &self.session_id
    }
}

pub struct AppSession {
    session: Session,
}

impl AppSession {
    pub fn new(session: Session) -> Self {
        Self { session }
    }

    pub fn id(&self) -> &str {
        self.session.id()
    }

    pub fn last_session_id(&self) -> Option<&str> {
        self.session.old_session_id.as_deref()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn replace_session_for_test(&mut self, session: Session) {
        self.session = session;
    }
}
