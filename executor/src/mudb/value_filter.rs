/* TODO: manage stack storage for recursion
 * recursive functions: eq, s_in, nin, validate/validate_map
 */

use super::error::{self, JsonCommandError::*, JsonCommandResult};

use serde::{Deserialize, Serialize};
use serde_json::Value::{self as JsonValue, Array, Bool, Null, Number, Object, String};

fn validate(filter: &JsonValue) -> JsonCommandResult<()> {
    // Operators just rise as Object.
    match filter {
        Object(map) => {
            for (key, val) in map {
                validate_items(key, val)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// *Command*
///
/// `$eq`, `$ne`, `$gt`, `$gte`, `$lt`, `lte`, `$in`, `$nin`
///
/// *Expected JsonValue*
///
/// `$eq`, `$ne` support all Values
/// `$gt`, `$gte`, `$lt`, `lte` should be just `Number`
/// `$in`, `$nin` should be just `Array`
fn validate_items(key: &str, value: &JsonValue) -> JsonCommandResult<()> {
    match (key, value) {
        ("$gt" | "$gte" | "$lt" | "$lte", Number(_)) | ("$in" | "$nin", Array(_)) => Ok(()),

        ("$gt" | "$gte" | "$lt" | "$lte", _) => Err(ExpectNum),

        ("$in" | "$nin", _) => Err(ExpectArr),

        // "$eq", "$ne" or sth else...
        (_, v) => validate(v),
    }
}

/// Matches values that filter is subset of it.
/// It's start point of filtering
///
/// Note: empty filter will returns `true`
///
/// # Panics
///
/// Panics if one of the gt, gte, lt, lte, in, nin make panics.
///
fn eq(value: &JsonValue, filter: &JsonValue) -> bool {
    match (value, filter) {
        (Null, Null) => true,

        (Bool(_), Bool(_)) | (Number(_), Number(_)) | (String(_), String(_)) => filter == value,

        (_, Object(f_map)) => f_map.into_iter().all(|(key, f_val)| match key.as_ref() {
            "$eq" => eq(value, f_val),
            "$ne" => ne(value, f_val),
            "$gt" => gt(value, f_val),
            "$gte" => gte(value, f_val),
            "$lt" => lt(value, f_val),
            "$lte" => lte(value, f_val),
            "$in" => s_in(value, f_val),
            "$nin" => nin(value, f_val),
            _ => match value {
                Object(v_map) => match v_map.get(key) {
                    Some(v_val) => eq(v_val, f_val),
                    None => false,
                },
                _ => false,
            },
        }),

        (Array(v_vec), Array(f_vec)) => f_vec
            .iter()
            .all(|f_val| v_vec.iter().any(|v_val| eq(v_val, f_val))),

        _ => false,
    }
}

/// Matches values that are greater than a specified value
///
/// # Panics
///
/// Panics if `filter` wan not `Number`.
///
fn gt(value: &JsonValue, filter: &JsonValue) -> bool {
    if let (Some(v), Some(f)) = (value.as_i64(), filter.as_i64()) {
        v > f
    } else if let (Some(v), Some(f)) = (value.as_u64(), filter.as_u64()) {
        v > f
    } else if let (Some(v), Some(f)) = (value.as_f64(), filter.as_f64()) {
        v > f
    } else if let Number(_) = filter {
        false
    } else {
        panic!(
            "gt: filter should be Number and \n\
            validate() should be called before gt() to avoid panic."
        )
    }
}

/// Matches values that are greater than or equal to a specified value.
///
/// # Panics
///
/// Panics if `filter` wan not `Number`.
///
fn gte(value: &JsonValue, filter: &JsonValue) -> bool {
    match (value, filter) {
        (Number(_), Number(_)) => gt(value, filter) | (value == filter),
        (_, Number(_)) => false,
        _ => panic!(
            "gte: filter should be Number and \n\
            validate() should be called before gte() to avoid panic."
        ),
    }
}

/// Matches values that are less than a specified value.
///
/// # Panics
///
/// Panics if `filter` wan not `Number`.
///
fn lt(value: &JsonValue, filter: &JsonValue) -> bool {
    if let (Some(v), Some(f)) = (value.as_i64(), filter.as_i64()) {
        v < f
    } else if let (Some(v), Some(f)) = (value.as_u64(), filter.as_u64()) {
        v < f
    } else if let (Some(v), Some(f)) = (value.as_f64(), filter.as_f64()) {
        v < f
    } else if let Number(_) = filter {
        false
    } else {
        panic!(
            "lt: filter should be Number and \n\
            validate() should be called before lt() to avoid panic."
        )
    }
}

/// Matches values that are less than or equal to a specified value.
///
/// # Panics
///
/// Panics if `filter` wan not `Number`.
///
fn lte(value: &JsonValue, filter: &JsonValue) -> bool {
    match (value, filter) {
        (Number(_), Number(_)) => lt(value, filter) | (value == filter),
        (_, Number(_)) => false,
        _ => panic!(
            "lte: filter should be Number and \n\
            validate() should be called before lte() to avoid panic."
        ),
    }
}

/// Matches all values that are not equal to a specified value.
fn ne(value: &JsonValue, filter: &JsonValue) -> bool {
    !eq(value, filter)
}

/// Matches any of the values specified in an array.
///
/// # Panics
///
/// Panics if `filter` wan not `Array`.
///
fn s_in(value: &JsonValue, filter: &JsonValue) -> bool {
    match filter {
        Array(f_vec) => f_vec.iter().any(|f_vec_value| eq(value, f_vec_value)),
        _ => panic!(
            "in: filter should be an Array and \n\
            validate() should be called before in() to avoid panic."
        ),
    }
}

/// Matches none of the values specified in an array.
///
/// # Panics
///
/// Panics if `filter` wan not `Array`.
///
fn nin(value: &JsonValue, filter: &JsonValue) -> bool {
    match filter {
        Array(f_vec) => f_vec.iter().all(|f_vec_value| ne(value, f_vec_value)),
        _ => panic!(
            "nin: filter should be an Array and \n\
            validate() should be called before nin() to avoid panic."
        ),
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValueFilter(JsonValue);

impl ValueFilter {
    pub fn eval(&self, doc: &JsonValue) -> bool {
        eq(doc, &self.0)
    }

    pub fn none() -> Self {
        // equal to: Self(json!({}))
        Self(Object(serde_json::Map::new()))
    }
}

impl TryFrom<JsonValue> for ValueFilter {
    type Error = error::Error;
    fn try_from(jv: JsonValue) -> Result<Self, Self::Error> {
        validate(&jv)?;
        Ok(Self(jv))
    }
}

impl TryFrom<std::string::String> for ValueFilter {
    type Error = error::Error;
    fn try_from(s: std::string::String) -> Result<Self, Self::Error> {
        let json_v: JsonValue = serde_json::from_str(&s)?;
        Self::try_from(json_v)
    }
}

impl TryFrom<&str> for ValueFilter {
    type Error = error::Error;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let json_v: JsonValue = serde_json::from_str(s)?;
        Self::try_from(json_v)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    fn init_doc() -> JsonValue {
        json!({
            "code": 200,
            "success": true,
            "payload": {
                "features": [
                    "serde",
                    "json"
                ]
            }
        })
    }

    #[test]
    fn none_r_empty_object() {
        let none_filter = ValueFilter::none();
        assert_eq!(none_filter, ValueFilter::try_from(json!({})).unwrap())
    }

    #[test]
    fn validate_r_ok_w_happened() {
        let filter = json!({
            "$in": ["h", "e", "l"]
        });
        assert_eq!(validate(&filter), Ok(()));

        let filter = json!({
            "payload": {
                "features": { "$eq": [ "serde", "json" ] }
            }
        });
        assert_eq!(validate(&filter), Ok(()));

        let filter = json!({
            "$gt": 5
        });
        assert_eq!(validate(&filter), Ok(()));

        let filter = json!({
            "payload": {
                "features": { "$gt": 5 }
            }
        });
        assert_eq!(validate(&filter), Ok(()));

        let filter = json!({});
        assert_eq!(filter.to_string(), "{}".to_string());
        assert_eq!(validate(&filter), Ok(()));

        let filter = json!("");
        assert_eq!(filter.to_string(), "\"\"".to_string());
        assert_eq!(validate(&filter), Ok(()));
    }

    #[test]
    fn validate_r_err_query_filter_w_happened() {
        let filter = json!({
            "$in": 5
        });
        assert_eq!(validate(&filter), Err(ExpectArr));

        let filter = json!({
            "payload": {
                "features": { "$lte": [ "serde", "json" ] }
            }
        });
        assert_eq!(validate(&filter), Err(ExpectNum));

        let filter = json!({
            "$gt": "5"
        });
        assert_eq!(validate(&filter), Err(ExpectNum));

        let filter = json!({
            "payload": {
                "features": { "$nin": 5 }
            }
        });
        assert_eq!(validate(&filter), Err(ExpectArr));
    }

    #[test]
    fn eq_r_true_w_happened_without_operation() {
        let doc = init_doc();

        let filter1 = json!({
            "payload": {
                "features": [
                    "serde",
                    "json"
                ]
            }
        });

        let filter2 = json!({
            "code": 200,
            "success": true,
        });

        let filter3 = json!({
            "payload": {
                "features": [
                    "serde",
                ]
            }
        });

        let filter4 = json!({});

        assert!(eq(&doc, &filter1));
        assert!(eq(&doc, &filter2));
        assert!(eq(&doc, &filter3));
        assert!(eq(&doc, &filter4));

        let none_object_doc = json!("none object");
        assert!(eq(&none_object_doc, &filter4));
    }

    #[test]
    fn eq_r_true_w_happened_with_operation() {
        let doc = init_doc();

        // code: 200

        // gt
        let gt_filter = json!({
            "code": { "$gt": 100 },
        });
        // gte
        let gte_filter = json!({
            "code": { "$gte": 200 },
        });
        // lt
        let lt_filter = json!({
            "code": { "$lt": 300 },
        });
        // lte
        let lte_filter = json!({
            "code": { "$lte": 300 },
            "success": true,
        });
        // eq
        let eq_filter = json!({
            "success": { "$eq": true }, // false filter, "success" is true
        });
        // ne
        let ne_filter = json!({
            "code": { "$ne": 300 },
        });

        assert!(eq(&doc, &gt_filter));
        assert!(eq(&doc, &gte_filter));
        assert!(eq(&doc, &lt_filter));
        assert!(eq(&doc, &lte_filter));
        assert!(eq(&doc, &eq_filter));
        assert!(eq(&doc, &ne_filter));
    }

    #[test]
    fn eq_r_false_w_happened_without_operation() {
        let doc = init_doc();

        // not found filter
        let filter = json!({
            "payload": {
                "features": [
                    "not_found_value",
                ]
            }
        });

        assert!(!eq(&filter, &doc));
    }

    #[test]
    fn eq_r_false_w_happened_with_operation() {
        let doc = init_doc();

        // gt
        let gt_filter = json!({
            "code": { "$gt": 300 },
        });
        // gte
        let gte_filter = json!({
            "code": { "$gte": 200 },
            "success": false, // false filter, "success" is true
        });
        // lt
        let lt_filter = json!({
            "code": { "$lt": 100 },
        });
        // lte
        let lte_filter = json!({
            "code": { "$lte": 100 },
        });
        // eq
        let eq_filter = json!({
            "success": { "$eq": false }, // false filter, "success" is true
        });
        // ne
        let ne_filter = json!({
            "code": { "$ne": 200 },
        });

        assert!(!eq(&doc, &gt_filter));
        assert!(!eq(&doc, &gte_filter));
        assert!(!eq(&doc, &lt_filter));
        assert!(!eq(&doc, &lte_filter));
        assert!(!eq(&doc, &eq_filter));
        assert!(!eq(&doc, &ne_filter));
    }

    #[test]
    fn ne_r_check_w_happened_without_operation() {
        let doc = init_doc();

        let filter1 = json!({
            "payload": {
                "features": [
                    "serde",
                    "json"
                ]
            }
        });

        let filter2 = json!({
            "code": 400, // false filter, code is 200
        });

        let filter3 = json!({
            "payload": {
                "features": [
                    "serde",
                ]
            }
        });

        assert!(!ne(&doc, &filter1));
        assert!(ne(&doc, &filter2));
        assert_eq!(ne(&doc, &filter3), !eq(&doc, &filter3));
    }

    #[test]
    fn gt_r_true_w_happened() {
        let filter = json!(1);
        let value = json!(2);

        let res = gt(&value, &filter);
        assert!(res)
    }

    #[test]
    fn gt_r_false_w_happened() {
        // less than
        let filter = json!(5);
        let value = json!(2);
        let res = gt(&value, &filter);
        assert!(!res);

        // equal
        let filter = json!(2);
        let value = json!(2);
        let res = gt(&value, &filter);
        assert!(!res);

        // not same type
        let filter = json!(2);
        let value = json!({
            "item": 1
        });

        let res = gt(&value, &filter);
        assert!(!res);
    }

    #[test]
    fn gte_r_true_w_happened() {
        // greater than
        let filter = json!(1);
        let value = json!(2);
        let res = gte(&value, &filter);
        assert!(res);

        // equal
        let filter = json!(2);
        let value = json!(2);
        let res = gte(&value, &filter);
        assert!(res);
    }

    #[test]
    fn gte_r_false_w_happened() {
        // less than
        let filter = json!(5);
        let value = json!(2);
        let res = gte(&value, &filter);
        assert!(!res);

        // not same type
        let filter = json!(2);
        let value = json!({
            "item": 2
        });

        let res = gte(&value, &filter);
        assert!(!res);
    }

    #[test]
    fn lt_r_true_w_happened() {
        let filter = json!(2);
        let value = json!(1);

        let res = lt(&value, &filter);
        assert!(res)
    }

    #[test]
    fn lt_r_false_w_happened() {
        // greater than
        let value = json!(5);
        let filter = json!(2);
        let res = lt(&value, &filter);
        assert!(!res);

        // equal
        let value = json!(2);
        let filter = json!(2);
        let res = lt(&value, &filter);
        assert!(!res);

        // not same type
        let filter = json!(1);
        let value = json!({
            "item": 2
        });

        let res = lt(&value, &filter);
        assert!(!res);
    }

    #[test]
    fn lte_r_true_w_happened() {
        // less than
        let filter = json!(2);
        let value = json!(1);
        let res = lte(&value, &filter);
        assert!(res);

        // equal
        let filter = json!(2);
        let value = json!(2);
        let res = lte(&value, &filter);
        assert!(res);
    }

    #[test]
    fn lte_r_false_w_happened() {
        // greater than
        let filter = json!(2);
        let value = json!(5);
        let res = lte(&value, &filter);
        assert!(!res);

        // not same type
        let filter = json!(2);
        let value = json!({
            "item": 2
        });

        let res = lte(&value, &filter);
        assert!(!res);
    }

    #[test]
    fn in_r_true_w_happened() {
        let value = json!(5);
        let filter = json!([1, 3, 5, 7]);
        let res = s_in(&value, &filter);
        assert!(res);
    }

    #[test]
    fn in_r_false_w_happened() {
        let value = json!(10);
        let filter = json!([1, 3, 5, 7]);
        let res = s_in(&value, &filter);
        assert!(!res);
    }

    #[test]
    fn nin_r_true_w_happened() {
        let value = json!(10);
        let filter = json!([1, 3, 5, 7]);
        let res = nin(&value, &filter);
        assert!(res);
    }

    #[test]
    fn nin_r_false_w_happened() {
        let value = json!(5);
        let filter = json!([1, 3, 5, 7]);
        let res = nin(&value, &filter);
        assert!(!res);
    }
}
