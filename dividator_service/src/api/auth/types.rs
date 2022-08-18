use rocket::form::FromFormField;
use rocket_okapi::JsonSchema;
use std::fmt;

/// Permissions that session could have
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum Permission {
    /// All included
    Admin,
}

/// Action that encoded in LNUrl
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, FromFormField, JsonSchema)]
pub enum LnAuthAction {
    Register,
    Login,
    Link,
    Auth,
}

impl fmt::Display for LnAuthAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LnAuthAction::Register => write!(f, "register"),
            LnAuthAction::Login => write!(f, "login"),
            LnAuthAction::Link => write!(f, "link"),
            LnAuthAction::Auth => write!(f, "auth"),
        }
    }
}
