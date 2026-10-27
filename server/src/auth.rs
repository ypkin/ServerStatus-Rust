use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    RequestPartsExt,
};
use axum_extra::{
    headers::{authorization::Basic, Authorization},
    TypedHeader,
};
use serde::{Deserialize, Serialize};

use crate::G_CONFIG;

#[derive(Debug, Serialize, Deserialize)]
pub struct BasicAuth {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminAuth(BasicAuth);
#[derive(Debug, Serialize, Deserialize)]
pub struct HostAuth(BasicAuth);

impl<S> FromRequestParts<S> for BasicAuth
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract the token from the authorization header
        let TypedHeader(Authorization(basic_auth)) = parts
            .extract::<TypedHeader<Authorization<Basic>>>()
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED.into_response())?;

        Ok(BasicAuth {
            username: basic_auth.username().into(),
            password: basic_auth.password().into(),
        })
    }
}

impl<S> FromRequestParts<S> for AdminAuth
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let unauth = (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, r#"Basic realm="Restricted""#)],
            StatusCode::UNAUTHORIZED.as_str(),
        )
            .into_response();

        // Extract the token from the authorization header
        let TypedHeader(Authorization(basic_auth)) = parts
            .extract::<TypedHeader<Authorization<Basic>>>()
            .await
            .map_err(|_| unauth)?;

        let mut auth_ok = false;
        if let Some(cfg) = G_CONFIG.get() {
            //
            auth_ok = cfg.admin_auth(basic_auth.username(), basic_auth.password());
        }
        if !auth_ok {
            return Err((
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, r#"Basic realm="Restricted""#)],
                StatusCode::UNAUTHORIZED.as_str(),
            )
                .into_response());
        }

        Ok(AdminAuth(BasicAuth {
            username: basic_auth.username().into(),
            password: basic_auth.password().into(),
        }))
    }
}

impl<S> FromRequestParts<S> for HostAuth
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract the token from the authorization header
        let TypedHeader(Authorization(basic_auth)) = parts
            .extract::<TypedHeader<Authorization<Basic>>>()
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED.into_response())?;

        let mut group_auth = false;
        if let Some(ssr_auth) = parts.headers.get("ssr-auth").and_then(|header| header.to_str().ok()) {
            group_auth = "group".eq(ssr_auth);
        }
        let auth_ok = if group_auth {
            crate::admin::check_group_auth(basic_auth.username(), basic_auth.password())
        } else {
            crate::admin::check_host_auth(basic_auth.username(), basic_auth.password())
        };
        if !auth_ok {
            return Err(StatusCode::UNAUTHORIZED.into_response());
        }

        Ok(HostAuth(BasicAuth {
            username: basic_auth.username().into(),
            password: basic_auth.password().into(),
        }))
    }
}
