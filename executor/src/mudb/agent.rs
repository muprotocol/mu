//! Manager
//! purpose is caching database

use mailbox_processor::{callback::CallbackMailboxProcessor, ReplyChannel};
use std::{collections::HashMap, fmt};

use super::{
    config::ConfigInner,
    db::Db,
    doc_filter::DocFilter,
    error::ManagerMailBoxError,
    table::Table,
    types::{Indexes, KeyFilter, KfBy, DB_DESCRIPTION_TABLE, MANAGER_DB},
    Error, Result,
};

// TODO: find a better name
#[derive(Clone)]
pub struct Agent {
    /// databases_description_table
    ddt: Table,
    /// mailbox
    mb: MailBox,
}

impl fmt::Debug for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Manager").field("ddt", &self.ddt).finish()
    }
}

impl Agent {
    pub async fn new() -> Result<Self> {
        ::tokio::task::spawn_blocking(Self::sync_new).await?
    }

    fn sync_new() -> Result<Self> {
        let conf = ConfigInner {
            database_id: MANAGER_DB.into(),
            ..Default::default()
        };
        let db = Db::open(conf)?;

        let indexes = Indexes {
            pk_attr: "database_id".into(),
            sk_attr_list: vec![],
        };

        // TODO: sync ddt to filesystem
        let ddt =
            db.get_or_create_table_if_not_exist(indexes, DB_DESCRIPTION_TABLE.try_into().unwrap())?;

        // TODO: consider buffer_size 100
        let mb = CallbackMailboxProcessor::start(step, HashMap::new(), 100);

        Ok(Self { ddt, mb })
    }

    /// Attempts to exclusively open the database, failing if it already exists
    pub async fn create_db(&self, x: ConfigInner) -> Result<()> {
        self.mb
            .post_and_reply(|tx| Message::CreateDb(self.clone(), x, tx))
            .await
            .map_err(ManagerMailBoxError::CreateDb)?
    }

    pub async fn drop_db(&self, x: &str) -> Result<()> {
        self.mb
            .post_and_reply(|tx| Message::DropDb(self.clone(), x.into(), tx))
            .await
            .map_err(ManagerMailBoxError::DropDb)?
    }

    /// get db from cache or open it from file system if has not cached.
    pub async fn get_db(&self, x: &str) -> Result<Db> {
        self.mb
            .post_and_reply(|tx| Message::GetDb(self.clone(), x.into(), tx))
            .await
            .map_err(ManagerMailBoxError::GetDb)?
    }

    pub fn is_db_exists(&self, x: &str) -> Result<bool> {
        let x = !self
            .ddt
            .query_by_key(KeyFilter::PK(KfBy::Exact(x.into())))?
            .is_empty();

        Ok(x)
    }

    /// read database config from database descriptiont table
    pub fn get_db_conf(&self, x: &str) -> Result<Option<ConfigInner>> {
        let y = self
            .ddt
            .query_by_key(KeyFilter::PK(KfBy::Exact(x.into())))?
            .pop()
            .map(|(_, doc)| doc.try_into().unwrap());

        Ok(y)
    }

    pub fn query_db_by_prefix(&self, x: &str) -> Result<Vec<String>> {
        let y = self
            .ddt
            .query_by_key(KeyFilter::PK(KfBy::Prefix(x.into())))?
            .into_iter()
            .map(|(name, _)| name.into())
            .collect();

        Ok(y)
    }

    pub async fn get_cache(&self) -> Result<State> {
        self.mb
            .post_and_reply(Message::GetCache)
            .await
            .map_err(ManagerMailBoxError::GetCache)
            .map_err(Into::into)
    }
}

type Rcr<T> = ReplyChannel<Result<T>>;

enum Message {
    CreateDb(Agent, ConfigInner, Rcr<()>),
    DropDb(Agent, String, Rcr<()>),
    GetDb(Agent, String, Rcr<Db>),
    GetCache(ReplyChannel<State>),
}

type State = HashMap<String, Db>;
type MailBox = CallbackMailboxProcessor<Message>;

async fn step(_: MailBox, msg: Message, mut state: State) -> State {
    macro_rules! flatten_result {
        ($join_res:expr, $rcr:expr, $f:expr) => {
            match $join_res {
                Ok(res) => match res {
                    Ok(x) => {
                        $f(&x);
                        $rcr.reply(Ok(x))
                    }
                    Err(e) => $rcr.reply(Err(e)),
                },
                Err(join_e) => $rcr.reply(Err(join_e.into())),
            }
        };
    }

    match msg {
        Message::CreateDb(manager, conf, rcr) => {
            let join_res = ::tokio::task::spawn_blocking(move || create_db(manager, conf)).await;
            flatten_result!(join_res, rcr, |_| ())
        }
        Message::DropDb(manager, name, rcr) => {
            state.remove(&name);
            let name_clone = name.clone();
            let join_res =
                ::tokio::task::spawn_blocking(move || drop_db(manager, &name_clone)).await;
            flatten_result!(join_res, rcr, |_| ())
        }
        Message::GetDb(manager, name, rcr) => match state.get(&name).map(Clone::clone) {
            Some(db) => rcr.reply(Ok(db)),
            _ => {
                let join_res = ::tokio::task::spawn_blocking(move || open_db(manager, &name)).await;
                flatten_result!(join_res, rcr, |x: &Db| state
                    .insert(x.id.clone(), x.clone()))
            }
        },
        Message::GetCache(rcr) => rcr.reply(state.clone()),
    };
    state
}

fn create_db(agent: Agent, conf: ConfigInner) -> Result<()> {
    if agent.is_db_exists(&conf.database_id)? {
        Err(Error::DbAlreadyExist(conf.database_id))
    } else {
        let db = Db::open(conf)?;
        agent.ddt.insert_one(db.conf.into())?;
        Ok(())
    }
}

fn drop_db(agent: Agent, db_name: &str) -> Result<()> {
    if agent.is_db_exists(db_name)? {
        let conf = ConfigInner {
            database_id: db_name.into(),
            ..Default::default()
        };
        // set temporary true, to remove db after drop it
        let db = sled::Config::from(conf).temporary(true).open()?;
        drop(db);
        agent.ddt.delete(
            KeyFilter::PK(KfBy::Exact(db_name.into())),
            DocFilter::none(),
        )?;

        Ok(())
    } else {
        Err(Error::DbDoseNotExist(db_name.into()))
    }
}

/// Opens a `MuDB` from filesystem base on the it's config.
fn open_db(agent: Agent, db_name: &str) -> Result<Db> {
    match agent.get_db_conf(db_name)? {
        Some(conf) => {
            let db = Db::open(conf)?;
            Ok(db)
        }
        _ => Err(Error::DbDoseNotExist(db_name.into())),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use assert_matches::assert_matches;
    use serial_test::serial;

    const TEST_DB: &str = "manager_test_db";

    async fn init() -> Agent {
        Agent::new().await.unwrap()
    }

    async fn seed(manager: &Agent) {
        let conf = ConfigInner {
            database_id: TEST_DB.into(),
            ..Default::default()
        };
        manager.create_db(conf).await.unwrap();
    }

    async fn seed_with(manager: &Agent, list: Vec<&str>) {
        for name in list {
            let conf = ConfigInner {
                database_id: name.into(),
                ..Default::default()
            };
            manager.create_db(conf).await.unwrap();
        }
    }

    async fn clean(manager: Agent) {
        let list = manager
            .ddt
            .query_by_key(KeyFilter::PK(KfBy::Prefix("".into())))
            .unwrap();

        for (name, _) in list {
            manager.drop_db(&name.to_string()).await.unwrap();
        }
    }

    #[tokio::test]
    #[serial]
    async fn just_one_new_manager() {
        let m1 = Agent::new().await;
        assert_matches!(m1, Ok(Agent { .. }));

        let m2 = Agent::new().await;
        assert_matches!(m2.err(), Some(Error::Sled(_)));
    }

    #[tokio::test]
    #[serial]
    async fn create_db_r_ok_and_check_file_system() {
        let manager = init().await;
        let db_id = "create_test_db";
        let conf = ConfigInner {
            database_id: db_id.into(),
            ..Default::default()
        };
        manager.create_db(conf).await.unwrap();

        let path = std::fs::read_dir("./mudb").unwrap();
        assert!(path
            .map(|res| res.unwrap())
            .any(|db_file| db_file.file_name().to_str() == Some(db_id)));

        // clean
        manager.drop_db(db_id).await.unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn create_db_r_err_already_exist_w_redundent() {
        let manager = init().await;
        seed(&manager).await;

        // redundent due to seed
        let conf = ConfigInner {
            database_id: TEST_DB.into(),
            ..Default::default()
        };
        let res = manager.create_db(conf).await;

        assert_eq!(res, Err(Error::DbAlreadyExist(TEST_DB.into())));

        clean(manager).await;
    }

    #[tokio::test]
    #[serial]
    async fn drop_db_r_ok_and_check_file_system() {
        let manager = init().await;
        seed(&manager).await;

        let res = manager.drop_db(TEST_DB).await;
        assert_eq!(res, Ok(()));

        let paths = std::fs::read_dir("./mudb").unwrap();

        assert!(paths
            .map(|res| res.unwrap())
            .all(|db_name| db_name.file_name().to_str() != Some(TEST_DB)));

        clean(manager).await;
    }

    #[tokio::test]
    #[serial]
    async fn drop_db_r_err_dose_not_exist() {
        let manager = init().await;

        let res = manager.drop_db(TEST_DB).await;
        assert_eq!(res, Err(Error::DbDoseNotExist(TEST_DB.into())))
    }

    #[tokio::test]
    #[serial]
    async fn exist_db_r_true() {
        let manager = init().await;
        seed(&manager).await;

        let res = manager.is_db_exists(TEST_DB);
        assert_eq!(res, Ok(true));

        clean(manager).await;
    }

    #[tokio::test]
    #[serial]
    async fn exist_db_r_false() {
        let manager = init().await;

        let res = manager.is_db_exists(TEST_DB);
        assert_eq!(res, Ok(false));
    }

    #[tokio::test]
    #[serial]
    async fn get_db_r_ok_db() {
        let manager = init().await;
        seed(&manager).await;

        {
            let db = manager.get_db(TEST_DB).await.unwrap();
            assert_eq!(db.conf.database_id, TEST_DB.to_string());
        }
        // db will drop

        clean(manager).await;
    }

    #[tokio::test]
    #[serial]
    async fn get_db_r_err_dose_not_exist() {
        let manager = init().await;

        let res = manager.get_db(TEST_DB).await;
        assert_eq!(res.err(), Some(Error::DbDoseNotExist(TEST_DB.into())));
    }

    #[tokio::test]
    #[serial]
    async fn get_cache_r_some() {
        let manager = init().await;
        seed(&manager).await;

        let _ = manager.get_db(TEST_DB).await.unwrap();

        assert!(manager
            .get_cache()
            .await
            .unwrap()
            .get(&TEST_DB.to_string())
            .is_some());

        clean(manager).await;
    }

    #[tokio::test]
    #[serial]
    async fn get_cache_r_none() {
        let manager = init().await;

        assert!(manager
            .get_cache()
            .await
            .unwrap()
            .get(&TEST_DB.to_string())
            .is_none());
    }

    #[tokio::test]
    #[serial]
    async fn query_db_by_prefix_r_ok_lists() {
        let manager = init().await;

        seed_with(
            &manager,
            vec!["a::b::db_1", "a::b::db_2", "a::c::db_3", "x::y::db_4"],
        )
        .await;

        assert_eq!(
            manager.query_db_by_prefix("a"),
            Ok(vec![
                "a::b::db_1".to_string(),
                "a::b::db_2".to_string(),
                "a::c::db_3".to_string()
            ])
        );

        assert_eq!(
            manager.query_db_by_prefix("a::b"),
            Ok(vec!["a::b::db_1".to_string(), "a::b::db_2".to_string()])
        );

        clean(manager).await;
    }
}
