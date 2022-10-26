use super::error::{self, JsonCommandError::*, JsonCommandResult};

use serde::{Deserialize, Serialize};
use serde_json::{
    json,
    Value::{self as JsonValue, Null, Object},
};

pub(crate) fn validate(update: &JsonValue) -> JsonCommandResult<()> {
    // Update values should be object.
    match update {
        Object(map) => {
            for (key, val) in map {
                validate_items(key, val)?;
            }
            Ok(())
        }
        _ => Err(ExpectObj),
    }
}

/// *Command*
///
/// `$set`, `$unset`, `$inc`, `$mul`
///
/// *Expected JsonValue*
///
/// `$set`, `$unset` expected `Object`
/// `$inc`, `$mul` expected `Object(Map<_, Number>)`
fn validate_items(key: &str, value: &JsonValue) -> JsonCommandResult<()> {
    match (key, value) {
        ("$set" | "$unset", Object(_)) => Ok(()),

        ("$inc" | "$mul", Object(map)) => map.iter().try_for_each(|(_, value)| {
            if value.is_number() {
                Ok(())
            } else {
                Err(ExpectNum)
            }
        }),

        ("$set" | "$unset" | "$inc" | "$mul", _) => Err(ExpectObj),

        _ => Err(InvalidOpr),
    }
}

// Used inside other functions like: set, unset, ...
fn set_inner(
    f_new_value: impl Fn(&JsonValue, JsonValue) -> JsonValue,
    doc: &mut JsonValue,
    update: &JsonValue,
) -> Vec<(String, JsonValue)> {
    match update {
        Object(map) => map
            .iter()
            .filter_map(|(k, v)| {
                let field = format!("/{}", k.replace('.', "/"));

                match doc.pointer_mut(&field) {
                    Some(tf) => {
                        *tf = f_new_value(tf, v.clone());
                        Some((k.clone(), tf.clone()))
                    }
                    None => None,
                }
            })
            .collect(),
        _ => panic!(
            "type error: that shouldn't happen, use\n\
            validate() to catch error"
        ),
    }
}

/// Sets the value of a field in an object and returns modified key/value.
///
/// # Usage
///
/// ```ignore
/// json!({ "$set": { "<field1>": <value1>, ... } })
/// ```
/// To specify a <field> in an embedded object or in an array, use dot notation.
///
/// ```ignore
/// json!({ "$set": { "shop.a_service.id": 1234 } })
/// ```
///
/// # Panics
///
/// Panics if `update` wan not `Object`.
///
/// # Example
///
/// ```ignore
/// let mut doc = json!({
///     "shop": {
///         "a_service": {
///             "id": 1
///         }
///     }
/// })
///
/// let value = json!({ "shop.a_service.id": 1234 });
/// let res = set(&mut doc, &value);
/// assert_eq!(res, vec![("shop.a_service.id".to_string(), json!(1234))]);
/// ```
fn set(doc: &mut JsonValue, update: &JsonValue) -> Vec<(String, JsonValue)> {
    set_inner(|_, update_v| update_v, doc, update)
}

/// Sets the value of a field `Null` in an object and returns unseted key/value.
///
/// # Usage
///
/// ```ignore
/// json!({ "$unset": { "quantity": "", "instock": "" } })
///
/// ```
///
/// The specified value in the `$unset` expression (i.e. `""`) does not
/// impact the operation
///
/// # Panics
///
/// Panics if `update` wan not `Object`.
///
fn unset(doc: &mut JsonValue, update: &JsonValue) -> Vec<(String, JsonValue)> {
    // TODO: It's not work like mongodb, cuz it's just set null not remove item.
    set_inner(|_, _| Null, doc, update)
}

/// Increments the value of the field by the specified amount and
/// returns modified key/value.
///
/// # Usage
///
/// ```ignore
/// json!({ "$inc": { "quantity": -2, "metrics.orders": 1 } })
/// ```
fn inc(doc: &mut JsonValue, update: &JsonValue) -> Vec<(String, JsonValue)> {
    // TODO: consider i/u/f_64 overflow!
    set_inner(
        |doc_v, update_v| {
            if let (Some(xp), Some(yp)) = (doc_v.as_i64(), update_v.as_i64()) {
                json!(xp + yp)
            } else if let (Some(xp), Some(yp)) = (doc_v.as_u64(), update_v.as_u64()) {
                json!(xp + yp)
            } else if let (Some(xp), Some(yp)) = (doc_v.as_f64(), update_v.as_f64()) {
                json!(xp + yp)
            } else {
                doc_v.clone()
            }
        },
        doc,
        update,
    )
}

/// Multiplies the value of the field by the specified amount and
/// returns modified key/value.
///
/// # Usage
///
/// ```ignore
/// json!({ "$mul": { "quantity": -2, "metrics.orders": 3 } })
/// ```
fn mul(doc: &mut JsonValue, update: &JsonValue) -> Vec<(String, JsonValue)> {
    // TODO: consider i/u/f_64 overflow!
    set_inner(
        |doc_v, update_v| {
            if let (Some(xp), Some(yp)) = (doc_v.as_i64(), update_v.as_i64()) {
                json!(xp * yp)
            } else if let (Some(xp), Some(yp)) = (doc_v.as_u64(), update_v.as_u64()) {
                json!(xp * yp)
            } else if let (Some(xp), Some(yp)) = (doc_v.as_f64(), update_v.as_f64()) {
                json!(xp * yp)
            } else {
                doc_v.clone()
            }
        },
        doc,
        update,
    )
}

fn update(doc: &mut JsonValue, update: &JsonValue) -> Vec<Vec<(String, JsonValue)>> {
    match update {
        Object(map) => map
            .iter()
            .map(|(k, v)| match k.as_str() {
                "$set" => set(doc, v),
                "$unset" => unset(doc, v),
                "$inc" => inc(doc, v),
                "$mul" => mul(doc, v),
                _ => panic!(
                    "command error: that shouldn't happen, use\n\
                    validate() beffor call exe(...) to catch error"
                ),
            })
            .filter(|x| !x.is_empty())
            .collect(),
        _ => panic!(
            "type error: that shouldn't happen, use\n\
            validate() beffor call exe(...) to catch error"
        ),
    }
}

pub type ChangedSections = Vec<Vec<(String, JsonValue)>>;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Updater(JsonValue);

impl Updater {
    pub fn update(&self, doc: &mut JsonValue) -> ChangedSections {
        update(doc, &self.0)
    }
}

impl TryFrom<JsonValue> for Updater {
    type Error = error::Error;
    fn try_from(jv: JsonValue) -> Result<Self, Self::Error> {
        validate(&jv)?;
        Ok(Self(jv))
    }
}

impl TryFrom<&str> for Updater {
    type Error = error::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        serde_json::from_str(value).map(Self).map_err(Into::into)
    }
}

impl TryFrom<String> for Updater {
    type Error = error::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        serde_json::from_str(&value).map(Self).map_err(Into::into)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    fn init_doc() -> JsonValue {
        json!({
            "code": 200,
            "code_2": 200,
            "code_3": 200,
            "code_4": 200,
            "code_5": [ 10, 11, 12, 13],
            "success": true,
            "payload": {
                "features": [
                    "serde",
                    "json"
                ]
            },
            "payload2": {
                "feature1": "serde",
                "feature2": "json",
            }
        })
    }

    #[test]
    fn validate_r_ok_w_happend() {
        // `$set`

        let update = json!({
            "$set": { "code": 100 }
        });
        assert_eq!(validate(&update), Ok(()));

        let update = json!({
            "$set": { }
        });
        assert_eq!(validate(&update), Ok(()));

        let update = json!({
            "$set": {
                "code": 100 ,
                "success": false
            }
        });
        assert_eq!(validate(&update), Ok(()));

        // `$unset`

        let update = json!({
            "$unset": { "code": "" }
        });
        assert_eq!(validate(&update), Ok(()));

        let update = json!({
            "$unset": { "code": 100 } // value (i.e. 100) dose not impact the operation
        });
        assert_eq!(validate(&update), Ok(()));

        // `$inc`

        let update = json!({
            "$inc": { "code": 2 }
        });
        assert_eq!(validate(&update), Ok(()));

        let update = json!({
            "$inc": { }
        });
        assert_eq!(validate(&update), Ok(()));

        let update = json!({
            "$inc": {
                "code": 2,
                "code2": 5
            }
        });
        assert_eq!(validate(&update), Ok(()));

        let update = json!({
            "$inc": { "code": -3 }
        });
        assert_eq!(validate(&update), Ok(()));

        // `$mul`

        let update = json!({
            "$mul": {
                "code": 2,
                "code2": 5
            }
        });
        assert_eq!(validate(&update), Ok(()));

        let update = json!({
            "$mul": { "code": -3 }
        });
        assert_eq!(validate(&update), Ok(()));
    }

    #[test]
    fn validate_r_err_w_happend() {
        // exe

        let update = json!(1000);
        assert_eq!(validate(&update), Err(ExpectObj));

        // `$set`

        let update = json!({
            "$set": "code"
        });
        assert_eq!(validate(&update), Err(ExpectObj));

        let update = json!({
            "$set": [1, 2, 3]
        });
        assert_eq!(validate(&update), Err(ExpectObj));

        // `$unset`

        let update = json!({
            "$unset": [1, 2, 3]
        });
        assert_eq!(validate(&update), Err(ExpectObj));

        // `$inc`

        let update = json!({
            "$inc": { "code": "hello" }
        });
        assert_eq!(validate(&update), Err(ExpectNum));

        let update = json!({
            "$inc": 2
        });
        assert_eq!(validate(&update), Err(ExpectObj));

        // `$mul`

        let update = json!({
            "$mul": { "code": "hello" }
        });
        assert_eq!(validate(&update), Err(ExpectNum));
    }

    #[test]
    fn set_r_modified_items() {
        let mut doc = init_doc();

        // simple

        let value = json!({ "code": 1234 });
        let res = set(&mut doc, &value);
        assert_eq!(*doc.get("code").unwrap(), json!(1234));
        assert_eq!(res, vec![("code".to_owned(), json!(1234))]);

        // inner

        let value = json!({ "payload2.feature1": "chrono" });
        let res = set(&mut doc, &value);
        assert_eq!(
            *doc.get("payload2").unwrap().get("feature1").unwrap(),
            json!("chrono")
        );
        assert_eq!(res, vec![("payload2.feature1".to_owned(), json!("chrono"))]);

        // arrary inner

        let value = json!({ "payload.features": [ 0 ] });
        let res = set(&mut doc, &value);
        assert_eq!(
            *doc.get("payload").unwrap().get("features").unwrap(),
            json!([0])
        );
        assert_eq!(res, vec![("payload.features".to_owned(), json!([0]))]);

        // multiple

        let value = json!({
            "payload2.feature2": 10,
            "success": false
        });
        let res = set(&mut doc, &value);
        assert_eq!(
            *doc.get("payload2").unwrap().get("feature2").unwrap(),
            json!(10)
        );
        assert_eq!(*doc.get("success").unwrap(), json!(false));
        assert_eq!(
            res,
            vec![
                ("payload2.feature2".to_owned(), json!(10)),
                ("success".to_owned(), json!(false))
            ]
        );

        // inside array

        let value = json!({ "code_5.0": 100,  "code_5.1": 110});
        let res = set(&mut doc, &value);
        assert_eq!(*doc.get("code_5").unwrap(), json!([100, 110, 12, 13]));
        assert_eq!(
            res,
            vec![
                ("code_5.0".to_owned(), json!(100)),
                ("code_5.1".to_owned(), json!(110))
            ]
        );
    }

    #[test]
    fn inc_r_modified_items() {
        let mut doc = init_doc();

        // add

        let value = json!({ "code": 2 });
        let res = inc(&mut doc, &value);
        assert_eq!(*doc.get("code").unwrap(), json!(202));
        assert_eq!(res, vec![("code".to_owned(), json!(202))]);

        // sub

        let value = json!({ "code_2": -10 });
        let res = inc(&mut doc, &value);
        assert_eq!(*doc.get("code_2").unwrap(), json!(190));
        assert_eq!(res, vec![("code_2".to_owned(), json!(190))]);

        // both

        let value = json!({ "code_3": 5, "code_4": -6 });
        let res = inc(&mut doc, &value);
        assert_eq!(*doc.get("code_3").unwrap(), json!(205));
        assert_eq!(*doc.get("code_4").unwrap(), json!(194));
        assert_eq!(
            res,
            vec![
                ("code_3".to_owned(), json!(205)),
                ("code_4".to_owned(), json!(194))
            ]
        );

        // inside array

        let value = json!({ "code_5.0": 1,  "code_5.1": 7});
        let res = inc(&mut doc, &value);
        assert_eq!(*doc.get("code_5").unwrap(), json!([11, 18, 12, 13]));
        assert_eq!(
            res,
            vec![
                ("code_5.0".to_owned(), json!(11)),
                ("code_5.1".to_owned(), json!(18))
            ]
        );
    }

    #[test]
    fn mul_r_modified_items() {
        let mut doc = init_doc();

        let value = json!({ "code": 2 });
        let res = mul(&mut doc, &value);
        assert_eq!(*doc.get("code").unwrap(), json!(400));
        assert_eq!(res, vec![("code".to_owned(), json!(400))]);

        let value = json!({ "code_2": -10 });
        let res = mul(&mut doc, &value);
        assert_eq!(*doc.get("code_2").unwrap(), json!(-2000));
        assert_eq!(res, vec![("code_2".to_owned(), json!(-2000))]);

        let value = json!({ "code_3": 5, "code_4": -6 });
        let res = mul(&mut doc, &value);
        assert_eq!(*doc.get("code_3").unwrap(), json!(1000));
        assert_eq!(*doc.get("code_4").unwrap(), json!(-1200));
        assert_eq!(
            res,
            vec![
                ("code_3".to_owned(), json!(1000)),
                ("code_4".to_owned(), json!(-1200))
            ]
        );

        let value = json!({ "code_5.0": 1,  "code_5.1": 7});
        let res = mul(&mut doc, &value);
        assert_eq!(*doc.get("code_5").unwrap(), json!([10, 77, 12, 13]));
        assert_eq!(
            res,
            vec![
                ("code_5.0".to_owned(), json!(10)),
                ("code_5.1".to_owned(), json!(77))
            ]
        );
    }

    #[test]
    fn unset_r_modified_items() {
        let mut doc = init_doc();

        let value = json!({ "code": "" });
        let res = unset(&mut doc, &value);
        assert_eq!(*doc.get("code").unwrap(), Null);
        assert_eq!(res, vec![("code".to_owned(), Null)]);

        let value = json!({ "code_2": 1 });
        let res = unset(&mut doc, &value);
        assert_eq!(*doc.get("code_2").unwrap(), Null);
        assert_eq!(res, vec![("code_2".to_owned(), Null)]);

        let value = json!({ "code_5.0": "",  "code_5.1": ""});
        let res = unset(&mut doc, &value);
        assert_eq!(*doc.get("code_5").unwrap(), json!([Null, Null, 12, 13]));
        assert_eq!(
            res,
            vec![("code_5.0".to_owned(), Null), ("code_5.1".to_owned(), Null)]
        );
    }

    #[test]
    fn exe_r_modified_items() {
        let mut doc = init_doc();

        // $set

        let value = json!({ "$set": { "payload2.feature1": "chrono" } });
        let res = update(&mut doc, &value);
        assert_eq!(
            *doc.get("payload2").unwrap().get("feature1").unwrap(),
            json!("chrono")
        );
        assert_eq!(
            res,
            vec![vec![("payload2.feature1".to_owned(), json!("chrono"))]]
        );

        // $unset

        let value = json!({ "$unset": { "code": "" }});
        let res = update(&mut doc, &value);
        assert_eq!(*doc.get("code").unwrap(), Null);
        assert_eq!(res, vec![vec![("code".to_owned(), Null)]]);

        // both

        let value = json!({
            "$set": { "payload.features": [ 0 ] },
            "$unset": { "code_2": 1 }
        });
        let res = update(&mut doc, &value);
        assert_eq!(
            *doc.get("payload").unwrap().get("features").unwrap(),
            json!([0])
        );
        assert_eq!(*doc.get("code_2").unwrap(), Null);
        assert_eq!(
            res,
            vec![
                vec![("payload.features".to_owned(), json!([0]))],
                vec![("code_2".to_owned(), Null)]
            ]
        );
    }
}
