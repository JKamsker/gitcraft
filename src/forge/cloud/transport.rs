//! One captured, verified login. Responses, redirects and pages never select a new host.
use super::{CloudAccount, Service, azure_login, bitbucket_login};
use crate::{
    connections,
    forge::{
        Hosting,
        http::{self, Login},
    },
};
use serde_json::Value;
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(in crate::forge) struct Api {
    pub service: Service,
    pub context: connections::Context,
    pub account: CloudAccount,
    pub viewer: String,
    pub viewer_id: String,
    repository: Option<(PathBuf, Hosting)>,
    base: String,
    login: Login,
    agent: ureq::Agent,
    began: Instant,
}
impl Api {
    pub fn authenticated(
        service: Service,
        account: CloudAccount,
        repository: Option<(&Path, &Hosting)>,
    ) -> Result<Self, String> {
        let context = connections::current();
        if let Some(error) = &context.error {
            return Err(error.clone());
        }
        account.validate(service)?;
        if !account.selected() {
            return Err(format!(
                "Choose and check this profile's {} account in Settings before opening its pull requests",
                service.name()
            ));
        }
        let server = service.server(&account.server)?;
        verify_repository_binding(service, &account, repository)?;
        let login = match service {
            Service::Azure => azure_login(&account)?,
            Service::Bitbucket => bitbucket_login(&account)?,
        };
        let base = match service {
            Service::Azure => server,
            Service::Bitbucket => "https://api.bitbucket.org/2.0".into(),
        };
        let mut api = Self {
            service,
            context,
            account,
            viewer: String::new(),
            viewer_id: String::new(),
            repository: repository.map(|(p, h)| (p.into(), h.clone())),
            base: base.clone(),
            login,
            agent: http::with_proxy(ureq::AgentBuilder::new(), &base)
                .redirects(0)
                .timeout(Duration::from_secs(30))
                .user_agent(concat!("gitcraft/", env!("CARGO_PKG_VERSION")))
                .build(),
            began: Instant::now(),
        };
        let identity = api.get(match service {
            Service::Azure => "_apis/connectionData?connectOptions=0",
            Service::Bitbucket => "user",
        })?;
        let (id, names) = match service {
            Service::Azure => (
                text(&identity, "/authenticatedUser/id"),
                vec![
                    text(&identity, "/authenticatedUser/uniqueName"),
                    text(&identity, "/authenticatedUser/id"),
                ],
            ),
            Service::Bitbucket => (
                text(&identity, "/uuid"),
                vec![
                    text(&identity, "/account_id"),
                    text(&identity, "/uuid"),
                    text(&identity, "/nickname"),
                    text(&identity, "/username"),
                ],
            ),
        };
        if id.is_empty()
            || !names
                .iter()
                .any(|name| !name.is_empty() && name.eq_ignore_ascii_case(&api.account.user))
            || (!api.account.subject.is_empty() && !id.eq_ignore_ascii_case(&api.account.subject))
        {
            return Err(format!(
                "The selected {} authentication belongs to a different account; no repository request was sent",
                service.name()
            ));
        }
        api.viewer = names
            .into_iter()
            .find(|n| !n.is_empty() && *n != id)
            .unwrap_or_else(|| id.clone());
        api.viewer_id = id;
        Ok(api)
    }
    fn verify_repository(&self) -> Result<(), String> {
        verify_repository_binding(
            self.service,
            &self.account,
            self.repository.as_ref().map(|(path, hosting)| (path.as_path(), hosting)),
        )
    }
    pub fn get(&self, endpoint: &str) -> Result<Value, String> {
        self.send("GET", endpoint, None)
    }
    pub fn send(
        &self,
        method: &str,
        endpoint: &str,
        payload: Option<&Value>,
    ) -> Result<Value, String> {
        let bytes = self.bytes(method, endpoint, payload, "application/json")?;
        if bytes.is_empty() {
            Ok(Value::Null)
        } else {
            serde_json::from_slice(&bytes)
                .map_err(|_| format!("{} returned invalid JSON", self.service.name()))
        }
    }
    pub fn bytes(
        &self,
        method: &str,
        endpoint: &str,
        payload: Option<&Value>,
        accept: &str,
    ) -> Result<Vec<u8>, String> {
        if endpoint.starts_with('/')
            || endpoint.contains("://")
            || endpoint.contains(['#', '\\'])
            || endpoint.chars().any(char::is_control)
            || endpoint
                .split('?')
                .next()
                .unwrap_or("")
                .split('/')
                .any(|p| p == ".." || p == ".")
        {
            return Err("Invalid provider API endpoint".into());
        }
        self.context.run(||{
            if let Some(error)=connections::current().error{return Err(error);}
            self.verify_repository()?;
            let remaining=Duration::from_secs(120).saturating_sub(self.began.elapsed());
            if remaining.is_zero(){return Err("Provider request deadline exceeded; reload to try again".into());}
            let url=format!("{}/{}",self.base,endpoint);
            let mut request=self.agent.request(method,&url).timeout(remaining.min(Duration::from_secs(30))).set("Accept",accept).set("Authorization",&self.login.header());
            if self.service==Service::Azure {request=request.set("X-TFS-FedAuthRedirect","Suppress");}
            let response=match payload {Some(body)=>request.set("Content-Type","application/json").send_string(&body.to_string()),None=>request.call()};
            let response=match response {
                Ok(r) if (200..300).contains(&r.status())=>r,
                Ok(r)=>return Err(format!("{} refused an automatic redirect (HTTP {}); refresh the repository address",self.service.name(),r.status())),
                Err(ureq::Error::Status(status,_))=>return Err(format!("{} request refused (HTTP {status}); check the selected account and repository permissions",self.service.name())),
                Err(ureq::Error::Transport(_))=>return Err(format!("Could not reach {} using the selected account",self.service.name())),
            };
            let mut bytes=Vec::new();response.into_reader().take(8*1024*1024+1).read_to_end(&mut bytes).map_err(|_|"Could not read the provider response")?;
            if bytes.len()>8*1024*1024{return Err("Provider response exceeds 8 MiB; narrow the request or use the forge page".into());}
            Ok(bytes)
        })
    }
    /// Follow only a service-owned next URL, never a new authentication destination.
    pub fn next(&self, url: &str, scope: &str) -> Result<String, String> {
        let prefix = format!("{}/{}", self.base, scope);
        let tail = url
            .strip_prefix(&prefix)
            .ok_or("The provider returned a page outside the selected repository")?;
        if !tail.is_empty() && !tail.starts_with(['?', '/']) {
            return Err("The provider returned an ambiguous page URL".into());
        }
        Ok(format!("{scope}{tail}"))
    }
}
pub(super) fn text(value: &Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .into()
}

fn verify_repository_binding(
    service: Service,
    account: &CloudAccount,
    repository: Option<(&Path, &Hosting)>,
) -> Result<(), String> {
        let Some((repo, hosting)) = repository else {
            return Ok(());
        };
        let remotes = crate::git::remotes(repo)?;
        let remote = remotes
            .iter()
            .find(|r| r.name == "origin")
            .or_else(|| remotes.first())
            .ok_or("The repository has no forge remote")?;
        let location = crate::forge::location(&remote.url);
        if location != Some((hosting.host.clone(), hosting.path.clone())) {
            return Err("The forge remote changed. Reload before continuing.".into());
        }
        let expected = match service {
            Service::Azure => {
                if hosting.host != "dev.azure.com" || hosting.web != "https://dev.azure.com" {
                    return Err(
                        "Azure requests require their canonical HTTPS service origin".into(),
                    );
                }
                let org = hosting
                    .path
                    .split('/')
                    .next()
                    .ok_or("Missing Azure organization")?;
                format!("https://dev.azure.com/{}", org.to_ascii_lowercase())
            }
            Service::Bitbucket => {
                if hosting.host != "bitbucket.org" || hosting.web != "https://bitbucket.org" {
                    return Err("Bitbucket requests require https://bitbucket.org".into());
                }
                "https://bitbucket.org".into()
            }
        };
        if service.server(&account.server)? != expected {
            return Err(
                "This profile selects a different forge server or Azure organization".into(),
            );
        }
        if remote.url.starts_with("http://") {
            return Err("Use the forge's HTTPS or SSH remote instead of an HTTP address".into());
        }
        Ok(())
}
