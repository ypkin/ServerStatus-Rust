//! PushPlus notifier (fork).
//!
//! PushPlus (pushplus.plus) delivers to WeChat / webhook / SMS depending on
//! the account's channel. This notifier mirrors the tgbot/wechat/email
//! shape: one template per event, rendered with the host in context.
//!
//! Unlike the other notifiers, its configuration is *runtime* state, not a
//! `&'static` startup reference: the admin panel can enable it, set the
//! token and edit templates live. `apply_pushplus_config` (called at
//! startup with the merged config.toml + admin.json values, and again on
//! every admin update) swaps the runtime config and re-registers the
//! templates. The notifier object itself is always registered; `notify`
//! no-ops while disabled or token-less, so enabling it in the panel needs
//! no restart.
//!
//! The token is never returned by any API: the admin GET endpoint only
//! reports `token_set`. It lives in `admin.json` (0600) or config.toml,
//! never in `stats.json`.

use anyhow::Result;
use log::{error, info};
use once_cell::sync::Lazy;
use reqwest;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Mutex;
use tokio::time::Duration;

use crate::jinja::{add_template, render_template};
use crate::notifier::{get_tag, Event, HostStat, NOTIFIER_HANDLE};

const KIND: &str = "pushplus";
const SEND_URL: &str = "https://www.pushplus.plus/send";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub online_tpl: String,
    #[serde(default)]
    pub offline_tpl: String,
    #[serde(default)]
    pub custom_tpl: String,
    #[serde(default)]
    pub traffic_tpl: String,
}

static RUNTIME: Lazy<Mutex<Config>> = Lazy::new(Default::default);

pub fn apply_pushplus_config(cfg: Config) {
    add_template(KIND, get_tag(&Event::NodeUp), cfg.online_tpl.clone());
    add_template(KIND, get_tag(&Event::NodeDown), cfg.offline_tpl.clone());
    add_template(KIND, get_tag(&Event::Custom), cfg.custom_tpl.clone());
    add_template(KIND, get_tag(&Event::Traffic), cfg.traffic_tpl.clone());
    *RUNTIME.lock().unwrap() = cfg;
}

pub struct PushPlus {
    http_client: reqwest::Client,
}

impl PushPlus {
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::new(),
        }
    }
}

impl Default for PushPlus {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::notifier::Notifier for PushPlus {
    fn kind(&self) -> &'static str {
        KIND
    }

    fn send_notify(&self, content: String) -> Result<()> {
        let cfg = RUNTIME.lock().unwrap().clone();
        if !cfg.enabled || cfg.token.is_empty() {
            return Ok(());
        }
        let title = if cfg.title.is_empty() { "ServerStatus".to_string() } else { cfg.title.clone() };
        let channel = if cfg.channel.is_empty() { "wechat".to_string() } else { cfg.channel.clone() };
        let token = cfg.token.clone();
        let http_client = self.http_client.clone();
        let handle = NOTIFIER_HANDLE.lock().unwrap().as_ref().unwrap().clone();
        handle.spawn(async move {
            match http_client
                .post(SEND_URL)
                .timeout(Duration::from_secs(10))
                .json(&json!({
                    "token": token,
                    "title": title,
                    "content": content,
                    "template": "html",
                    "channel": channel,
                }))
                .send()
                .await
            {
                Ok(resp) => {
                    info!("pushplus send resp => {resp:?}");
                }
                Err(err) => {
                    error!("pushplus send error => {err:?}");
                }
            }
        });
        Ok(())
    }

    fn notify(&self, e: &Event, stat: &HostStat) -> Result<()> {
        let cfg = RUNTIME.lock().unwrap().clone();
        if !cfg.enabled || cfg.token.is_empty() {
            return Ok(());
        }
        let content = render_template(
            self.kind(),
            get_tag(e),
            minijinja::context!(host => stat, config => cfg, ip_info => stat.ip_info, sys_info => stat.sys_info),
            true,
        )
        .unwrap_or_default();
        let content = match *e {
            Event::NodeUp | Event::NodeDown => {
                if content.is_empty() {
                    let verb = if matches!(*e, Event::NodeUp) { "恢复上线" } else { "掉线" };
                    let name = if stat.alias.is_empty() { stat.name.clone() } else { stat.alias.clone() };
                    format!("{name} ({}) {verb}", stat.name)
                } else {
                    content
                }
            }
            Event::Custom | Event::Traffic => {
                if content.is_empty() {
                    return Ok(());
                }
                content
            }
        };
        self.send_notify(content)
    }
}
