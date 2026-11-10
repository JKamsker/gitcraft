//! Bounded in-memory approval and opaque authentication receipt lifetimes.
use super::*;

static PLANS: OnceLock<Mutex<BTreeMap<u64, Arc<Plan>>>> = OnceLock::new();
pub(crate) fn register(plan: Plan) -> Vec<String> {
    static IDS: AtomicU64 = AtomicU64::new(1);
    let id = IDS.fetch_add(1, Ordering::Relaxed);
    let mut plans = PLANS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    while plans.len() >= 64 {
        plans.pop_first();
    }
    plans.insert(id, Arc::new(plan));
    vec![APPROVED.into(), id.to_string()]
}

pub(crate) fn registered(id: &str) -> Result<Arc<Plan>, String> {
    PLANS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&id.parse::<u64>().map_err(|_| "Reopen push preflight")?)
        .cloned()
        .ok_or_else(|| "This preview expired; reopen push preflight.".into())
}

const AUTH_ENDPOINT: &str = "[gitcraft-reviewed-auth:";
fn auth_endpoints() -> &'static Mutex<BTreeMap<u64, String>> {
    static ENDPOINTS: OnceLock<Mutex<BTreeMap<u64, String>>> = OnceLock::new();
    ENDPOINTS.get_or_init(Default::default)
}
pub(crate) fn remember_auth_endpoint(url: &str) -> String {
    static IDS: AtomicU64 = AtomicU64::new(1);
    let id = IDS.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut endpoints) = auth_endpoints().lock() {
        while endpoints.len() >= 64 {
            if let Some(first) = endpoints.keys().next().copied() {
                endpoints.remove(&first);
            }
        }
        endpoints.insert(id, url.into());
    }
    // The log/status carries an opaque receipt, never URL userinfo/query values.
    format!("Authentication failed for the reviewed push target. {AUTH_ENDPOINT}{id}]")
}
pub(crate) fn take_auth_endpoint(error: &str) -> Option<String> {
    let id = error
        .split_once(AUTH_ENDPOINT)?
        .1
        .strip_suffix(']')?
        .parse::<u64>()
        .ok()?;
    auth_endpoints().lock().ok()?.remove(&id)
}
pub(crate) fn is_reviewed_auth_error(error: &str) -> bool {
    error.contains(AUTH_ENDPOINT)
}
