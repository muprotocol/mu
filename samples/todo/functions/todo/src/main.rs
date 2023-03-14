use base64::{engine::general_purpose::STANDARD, Engine};
use musdk::*;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
struct Todo {
    title: String,
    done: bool,
    attachments: Vec<Attachment>,
}

#[derive(Deserialize, Serialize)]
struct Attachment {
    name: String,
    data: String,
}

struct UserId(String);

impl<'a> FromRequest<'a> for UserId {
    type Error = &'static str;

    fn from_request(req: &'a Request) -> std::result::Result<Self, Self::Error> {
        Ok(Self(
            req.headers
                .iter()
                .find(|h| h.name == "x-user-id")
                .ok_or("x-user-id header not found")?
                .value
                .to_string(),
        ))
    }
}

#[mu_functions]
mod greeting {
    use super::*;

    #[mu_function]
    fn get_all<'a>(ctx: &'a mut MuContext, user_id: UserId) -> Json<Vec<Todo>> {
        let mut db = ctx.db();
        let todos = db
            .scan("todos", user_id.0.clone(), 1000)
            .unwrap()
            .into_iter()
            .map(|(k, v)| read_todo(ctx, user_id.0.as_str(), k.0, v.0))
            .collect();

        Json(todos)
    }

    #[mu_function]
    fn get_todo<'a>(
        ctx: &'a mut MuContext,
        user_id: UserId,
        path_params: PathParams<'a>,
    ) -> Json<Option<Todo>> {
        let todo_id = path_params.get("title").unwrap().to_string();
        let key = format!("{}!!{todo_id}", user_id.0).into_bytes();
        let value = ctx.db().get("todos", &key).unwrap();
        Json(value.map(|v| read_todo(ctx, user_id.0.as_str(), key, v.0)))
    }

    #[mu_function]
    fn add_todo<'a>(ctx: &'a mut MuContext, user_id: UserId, todo: Json<Todo>) {
        let todo = todo.into_inner();
        let key = format!("{}!!{}", user_id.0, todo.title).into_bytes();
        let value = if todo.done { [1] } else { [0] };
        ctx.db().put("todos", key, value, false).unwrap();
        let mut storage = ctx.storage();
        for a in todo.attachments {
            storage
                .put(
                    "todo-attachments",
                    &format!("{}/{}/{}", user_id.0, todo.title, a.name),
                    &STANDARD.decode(a.data).unwrap(),
                )
                .unwrap();
        }
    }
}

fn read_todo(ctx: &mut MuContext, user_id: &str, key: Vec<u8>, value: Vec<u8>) -> Todo {
    let done = value[0] == 1;
    let title = String::from_utf8(key[user_id.as_bytes().len() + 2..].to_vec()).unwrap();

    let attachment_prefix = format!("{user_id}/{title}/");
    let mut storage = ctx.storage();
    let attachment_objects = storage
        .search_by_prefix("todo-attachments", attachment_prefix.as_str())
        .unwrap()
        .into_iter()
        .map(|o| o.key.into_owned())
        .collect::<Vec<_>>();
    let attachments = attachment_objects
        .into_iter()
        .map(|o| Attachment {
            data: STANDARD.encode(storage.get("todo-attachments", o.as_ref()).unwrap()),
            name: o.strip_prefix(&attachment_prefix).unwrap().to_string(),
        })
        .collect();

    Todo {
        attachments,
        title,
        done,
    }
}
