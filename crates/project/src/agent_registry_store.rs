use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, anyhow, bail};
use collections::HashMap;
use fs::Fs;
use futures::{AsyncReadExt, future::join_all};
use gpui::{
    App, AppContext as _, BackgroundExecutor, Context, Entity, FutureExt as _, Global,
    SharedString, Task, TaskExt,
};
use http_client::{AsyncBody, HttpClient, StatusCode};
use serde::Deserialize;
use settings::Settings as _;
use util::ResultExt;

use crate::{AgentId, DisableAiSettings};

const REGISTRY_URL: &str = "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";
pub const ORION_CODE_AGENT_ID: &str = "orion-code";
const ORION_CODE_NPM_PACKAGE: &str = "@orion-agents/orion-code";
const REFRESH_THROTTLE_DURATION: Duration = Duration::from_secs(60 * 60);
// Bound the full request lifecycle, including response body reads; the shared
// HTTP client only has a connect timeout.
const REGISTRY_FETCH_TIMEOUT: Duration = Duration::from_secs(30);
const REGISTRY_ICON_FETCH_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug)]
pub struct RegistryAgentMetadata {
    pub id: AgentId,
    pub name: SharedString,
    pub description: SharedString,
    pub version: SharedString,
    pub repository: Option<SharedString>,
    pub website: Option<SharedString>,
    pub icon_path: Option<SharedString>,
}

#[derive(Clone, Debug)]
pub struct RegistryBinaryAgent {
    pub metadata: RegistryAgentMetadata,
    pub targets: HashMap<String, RegistryTargetConfig>,
    pub supports_current_platform: bool,
}

#[derive(Clone, Debug)]
pub struct RegistryNpxAgent {
    pub metadata: RegistryAgentMetadata,
    pub package: SharedString,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct RegistryUnavailableAgent {
    pub metadata: RegistryAgentMetadata,
    pub reason: SharedString,
}

#[derive(Clone, Debug)]
pub enum RegistryAgent {
    Binary(RegistryBinaryAgent),
    Npx(RegistryNpxAgent),
    Unavailable(RegistryUnavailableAgent),
}

impl RegistryAgent {
    pub fn metadata(&self) -> &RegistryAgentMetadata {
        match self {
            RegistryAgent::Binary(agent) => &agent.metadata,
            RegistryAgent::Npx(agent) => &agent.metadata,
            RegistryAgent::Unavailable(agent) => &agent.metadata,
        }
    }

    pub fn id(&self) -> &AgentId {
        &self.metadata().id
    }

    pub fn name(&self) -> &SharedString {
        &self.metadata().name
    }

    pub fn description(&self) -> &SharedString {
        &self.metadata().description
    }

    pub fn version(&self) -> &SharedString {
        &self.metadata().version
    }

    pub fn repository(&self) -> Option<&SharedString> {
        self.metadata().repository.as_ref()
    }

    pub fn website(&self) -> Option<&SharedString> {
        self.metadata().website.as_ref()
    }

    pub fn icon_path(&self) -> Option<&SharedString> {
        self.metadata().icon_path.as_ref()
    }

    pub fn supports_current_platform(&self) -> bool {
        match self {
            RegistryAgent::Binary(agent) => agent.supports_current_platform,
            RegistryAgent::Npx(_) => true,
            RegistryAgent::Unavailable(_) => false,
        }
    }

    pub fn is_installable(&self) -> bool {
        !matches!(self, RegistryAgent::Unavailable(_)) && self.supports_current_platform()
    }

    pub fn unavailable_reason(&self) -> Option<&SharedString> {
        match self {
            RegistryAgent::Unavailable(agent) => Some(&agent.reason),
            RegistryAgent::Binary(_) | RegistryAgent::Npx(_) => None,
        }
    }
}

fn orion_code_registry_metadata(version: SharedString) -> RegistryAgentMetadata {
    RegistryAgentMetadata {
        id: AgentId::new(ORION_CODE_AGENT_ID),
        name: "Orion Code".into(),
        description: "Orion Studio's recommended local coding agent, connected over ACP.".into(),
        version,
        repository: None,
        website: None,
        icon_path: None,
    }
}

fn first_party_registry_agents(version_floor: Option<&semver::Version>) -> Vec<RegistryAgent> {
    match version_floor {
        Some(version) => vec![verified_orion_code_registry_agent(version)],
        None => vec![RegistryAgent::Unavailable(RegistryUnavailableAgent {
            metadata: orion_code_registry_metadata(SharedString::default()),
            reason: "Connect to the ACP Registry to fetch an Orion Code Preview, or continue with Orion Agent."
                .into(),
        })],
    }
}

fn verified_orion_code_registry_agent(version: &semver::Version) -> RegistryAgent {
    let version = version.to_string();
    RegistryAgent::Npx(RegistryNpxAgent {
        metadata: orion_code_registry_metadata(version.clone().into()),
        package: format!("{ORION_CODE_NPM_PACKAGE}@{version}").into(),
        args: Vec::new(),
        env: HashMap::default(),
    })
}

fn apply_orion_code_version_pin(
    agents: &mut Vec<RegistryAgent>,
    pinned_version: Option<&semver::Version>,
) {
    let Some(pinned_version) = pinned_version else {
        return;
    };
    let pinned_agent = verified_orion_code_registry_agent(pinned_version);
    if let Some(index) = agents
        .iter()
        .position(|agent| agent.id().as_ref() == ORION_CODE_AGENT_ID)
    {
        agents[index] = pinned_agent;
    } else {
        agents.push(pinned_agent);
    }
}

fn validate_orion_code_npx_binding(
    agent: &RegistryNpxAgent,
) -> std::result::Result<(), SharedString> {
    let package = agent.package.as_ref();
    let metadata_version = agent.metadata.version.as_ref();
    let package_version_prefix = format!("{ORION_CODE_NPM_PACKAGE}@");
    let Some(package_version) = package.strip_prefix(&package_version_prefix) else {
        let reason = if package == ORION_CODE_NPM_PACKAGE {
            format!(
                "Orion Code Registry package must include an exact semantic version: expected {ORION_CODE_NPM_PACKAGE}@{metadata_version}"
            )
        } else {
            format!(
                "Orion Code Registry entry must use the trusted package {ORION_CODE_NPM_PACKAGE}; received {package}"
            )
        };
        return Err(reason.into());
    };

    if semver::Version::parse(metadata_version).is_err() {
        return Err(format!(
            "Orion Code Registry metadata version must be an exact semantic version; received {metadata_version}"
        )
        .into());
    }

    if semver::Version::parse(package_version).is_err() {
        return Err(format!(
            "Orion Code Registry package version must be an exact semantic version; received {package_version}"
        )
        .into());
    }

    if package_version != metadata_version {
        return Err(format!(
            "Orion Code Registry package version {package_version} must exactly match metadata version {metadata_version}"
        )
        .into());
    }

    if !agent.args.is_empty() {
        return Err("Orion Code Registry package must not declare launcher arguments".into());
    }
    if !agent.env.is_empty() {
        return Err(
            "Orion Code Registry package must not declare distribution environment variables"
                .into(),
        );
    }

    Ok(())
}

fn enforce_orion_code_registry_binding(agent: RegistryAgent) -> RegistryAgent {
    if agent.id().as_ref() != ORION_CODE_AGENT_ID {
        return agent;
    }

    match agent {
        RegistryAgent::Npx(mut agent) => match validate_orion_code_npx_binding(&agent) {
            Ok(()) => {
                agent.metadata = orion_code_registry_metadata(agent.metadata.version.clone());
                RegistryAgent::Npx(agent)
            }
            Err(reason) => RegistryAgent::Unavailable(RegistryUnavailableAgent {
                metadata: orion_code_registry_metadata(SharedString::default()),
                reason,
            }),
        },
        RegistryAgent::Binary(_) => RegistryAgent::Unavailable(RegistryUnavailableAgent {
            metadata: orion_code_registry_metadata(SharedString::default()),
            reason: "Orion Code Preview must use the trusted exact-version npx distribution".into(),
        }),
        RegistryAgent::Unavailable(agent) => RegistryAgent::Unavailable(agent),
    }
}

fn should_replace_registry_agent(current: &RegistryAgent, candidate: &RegistryAgent) -> bool {
    if matches!(current, RegistryAgent::Unavailable(_)) {
        return !matches!(candidate, RegistryAgent::Unavailable(_));
    }
    if matches!(candidate, RegistryAgent::Unavailable(_)) {
        return false;
    }

    match (
        semver::Version::parse(current.version()),
        semver::Version::parse(candidate.version()),
    ) {
        (Ok(current), Ok(candidate)) => candidate > current,
        (Err(_), Ok(_)) => true,
        (Ok(_), Err(_)) | (Err(_), Err(_)) => false,
    }
}

fn merge_registry_agents(
    first_party_agents: Vec<RegistryAgent>,
    remote_agents: Vec<RegistryAgent>,
) -> Vec<RegistryAgent> {
    let mut merged = Vec::new();
    for remote_agent in remote_agents
        .into_iter()
        .map(enforce_orion_code_registry_binding)
    {
        if let Some(index) = merged
            .iter()
            .position(|agent: &RegistryAgent| agent.id() == remote_agent.id())
        {
            if should_replace_registry_agent(&merged[index], &remote_agent) {
                merged[index] = remote_agent;
            }
        } else {
            merged.push(remote_agent);
        }
    }

    let mut indices = merged
        .iter()
        .enumerate()
        .map(|(index, agent)| (agent.id().clone(), index))
        .collect::<HashMap<_, _>>();

    for first_party_agent in first_party_agents {
        if let Some(index) = indices.get(first_party_agent.id()).copied() {
            let remote_agent = &merged[index];
            let first_party_is_preferred = match (
                semver::Version::parse(remote_agent.version()),
                semver::Version::parse(first_party_agent.version()),
            ) {
                (_, _) if matches!(&first_party_agent, RegistryAgent::Unavailable(_)) => false,
                (Ok(remote), Ok(first_party)) => first_party >= remote,
                (Err(_), Ok(_)) => true,
                (Ok(_), Err(_)) | (Err(_), Err(_)) => false,
            };
            if first_party_is_preferred {
                merged[index] = first_party_agent;
            }
        } else {
            indices.insert(first_party_agent.id().clone(), merged.len());
            merged.push(first_party_agent);
        }
    }

    merged
}

#[derive(Clone, Debug)]
pub struct RegistryTargetConfig {
    pub archive: String,
    pub cmd: String,
    pub args: Vec<String>,
    pub sha256: Option<String>,
    pub env: HashMap<String, String>,
}

struct GlobalAgentRegistryStore(Entity<AgentRegistryStore>);

impl Global for GlobalAgentRegistryStore {}

pub struct AgentRegistryStore {
    fs: Arc<dyn Fs>,
    http_client: Arc<dyn HttpClient>,
    agents: Vec<RegistryAgent>,
    is_fetching: bool,
    fetch_error: Option<SharedString>,
    pending_refresh: Option<Task<()>>,
    last_refresh: Option<Instant>,
    orion_code_version_floor: Option<semver::Version>,
    orion_code_version_pin: Option<semver::Version>,
}

impl AgentRegistryStore {
    /// Initialize the global AgentRegistryStore.
    ///
    /// This loads the cached registry from disk. If the cache is empty but there
    /// are registry agents configured in settings, it will trigger a network fetch.
    /// Otherwise, call `refresh()` explicitly when you need fresh data
    /// (e.g., when opening the Agent Registry page).
    pub fn init_global(
        cx: &mut App,
        fs: Arc<dyn Fs>,
        http_client: Arc<dyn HttpClient>,
    ) -> Entity<Self> {
        if let Some(store) = Self::try_global(cx) {
            return store;
        }

        let store = cx.new(|cx| Self::new(fs, http_client, cx));
        cx.set_global(GlobalAgentRegistryStore(store.clone()));

        store.update(cx, |store, cx| store.refresh(cx));

        store
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalAgentRegistryStore>().0.clone()
    }

    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalAgentRegistryStore>()
            .map(|store| store.0.clone())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn init_test_global(cx: &mut App, agents: Vec<RegistryAgent>) -> Entity<Self> {
        let fs: Arc<dyn Fs> = fs::FakeFs::new(cx.background_executor().clone());
        let store = cx.new(|_cx| Self {
            fs,
            http_client: http_client::FakeHttpClient::with_404_response(),
            agents,
            is_fetching: false,
            fetch_error: None,
            pending_refresh: None,
            last_refresh: None,
            orion_code_version_floor: None,
            orion_code_version_pin: None,
        });
        cx.set_global(GlobalAgentRegistryStore(store.clone()));
        store
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_agents(&mut self, agents: Vec<RegistryAgent>, cx: &mut Context<Self>) {
        self.agents = agents;
        cx.notify();
    }

    pub fn agents(&self) -> &[RegistryAgent] {
        &self.agents
    }

    pub fn set_orion_code_version_floor(
        &mut self,
        version: &str,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let version = semver::Version::parse(version)
            .with_context(|| format!("invalid Orion Code verified version {version}"))?;
        if self
            .orion_code_version_floor
            .as_ref()
            .is_some_and(|floor| floor >= &version)
        {
            return Ok(());
        }

        self.orion_code_version_floor = Some(version);
        self.agents = merge_registry_agents(
            first_party_registry_agents(self.orion_code_version_floor.as_ref()),
            std::mem::take(&mut self.agents),
        );
        apply_orion_code_version_pin(&mut self.agents, self.orion_code_version_pin.as_ref());
        cx.notify();
        Ok(())
    }

    pub fn pin_orion_code_to_verified_version(
        &mut self,
        version: &str,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let version = semver::Version::parse(version)
            .with_context(|| format!("invalid Orion Code rollback version {version}"))?;
        self.orion_code_version_pin = Some(version);
        apply_orion_code_version_pin(&mut self.agents, self.orion_code_version_pin.as_ref());
        cx.notify();
        Ok(())
    }

    pub fn clear_orion_code_version_pin_and_refresh(&mut self, cx: &mut Context<Self>) {
        if self.orion_code_version_pin.take().is_none() {
            return;
        }
        cx.notify();
        self.refresh(cx);
    }

    pub fn agent(&self, id: &AgentId) -> Option<&RegistryAgent> {
        self.agents.iter().find(|agent| agent.id() == id)
    }

    pub fn is_fetching(&self) -> bool {
        self.is_fetching
    }

    pub fn fetch_error(&self) -> Option<SharedString> {
        self.fetch_error.clone()
    }

    /// Refresh the registry from the network.
    ///
    /// This will fetch the latest registry data and update the cache.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.pending_refresh.is_some() {
            return;
        }

        if DisableAiSettings::get_global(cx).disable_ai {
            return;
        }

        self.is_fetching = true;
        self.fetch_error = None;
        self.last_refresh = Some(Instant::now());
        cx.notify();

        let fs = self.fs.clone();
        let http_client = self.http_client.clone();
        let executor = cx.background_executor().clone();

        self.pending_refresh = Some(cx.spawn(async move |this, cx| {
            let result = match fetch_registry_index(http_client.clone(), &executor).await {
                Ok(data) => {
                    build_registry_agents(
                        fs.clone(),
                        http_client,
                        data.index,
                        data.raw_body,
                        true,
                        &executor,
                    )
                    .await
                }
                Err(error) => {
                    log::error!("AgentRegistryStore::refresh: fetch failed: {error:#}");
                    Err(error)
                }
            };

            this.update(cx, |this, cx| {
                this.pending_refresh = None;
                this.is_fetching = false;
                match result {
                    Ok(agents) => {
                        this.agents = merge_registry_agents(
                            first_party_registry_agents(this.orion_code_version_floor.as_ref()),
                            agents,
                        );
                        apply_orion_code_version_pin(
                            &mut this.agents,
                            this.orion_code_version_pin.as_ref(),
                        );
                        this.fetch_error = None;
                    }
                    Err(error) => {
                        this.fetch_error = Some(SharedString::from(format!("{error:#}")));
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// Refresh the registry if it hasn't been refreshed recently.
    ///
    /// This is useful to call when using a registry-based agent to check for
    /// updates without making too many network requests. The refresh is
    /// throttled to at most once per hour.
    pub fn refresh_if_stale(&mut self, cx: &mut Context<Self>) {
        let should_refresh = self
            .last_refresh
            .map(|last| last.elapsed() >= REFRESH_THROTTLE_DURATION)
            .unwrap_or(true);

        if should_refresh {
            self.refresh(cx);
        }
    }

    fn new(fs: Arc<dyn Fs>, http_client: Arc<dyn HttpClient>, cx: &mut Context<Self>) -> Self {
        let mut store = Self {
            fs: fs.clone(),
            http_client,
            agents: first_party_registry_agents(None),
            is_fetching: false,
            fetch_error: None,
            pending_refresh: None,
            last_refresh: None,
            orion_code_version_floor: None,
            orion_code_version_pin: None,
        };

        store.load_cached_registry(fs, store.http_client.clone(), cx);

        store
    }

    fn load_cached_registry(
        &mut self,
        fs: Arc<dyn Fs>,
        http_client: Arc<dyn HttpClient>,
        cx: &mut Context<Self>,
    ) {
        if DisableAiSettings::get_global(cx).disable_ai {
            return;
        }

        cx.spawn(async move |this, cx| -> Result<()> {
            let cache_path = registry_cache_path();
            if !fs.is_file(&cache_path).await {
                return Ok(());
            }

            let bytes = fs
                .load_bytes(&cache_path)
                .await
                .context("reading cached registry")?;
            let index: RegistryIndex =
                serde_json::from_slice(&bytes).context("parsing cached registry")?;

            let executor = cx.background_executor().clone();
            let agents =
                build_registry_agents(fs, http_client, index, bytes, false, &executor).await?;

            this.update(cx, |this, cx| {
                this.agents = merge_registry_agents(
                    first_party_registry_agents(this.orion_code_version_floor.as_ref()),
                    agents,
                );
                apply_orion_code_version_pin(
                    &mut this.agents,
                    this.orion_code_version_pin.as_ref(),
                );
                cx.notify();
            })?;

            Ok(())
        })
        .detach_and_log_err(cx);
    }
}

struct RegistryFetchResult {
    index: RegistryIndex,
    raw_body: Vec<u8>,
}

async fn fetch_registry_index(
    http_client: Arc<dyn HttpClient>,
    executor: &BackgroundExecutor,
) -> Result<RegistryFetchResult> {
    let (status, body) =
        fetch_url_body(http_client, REGISTRY_URL, REGISTRY_FETCH_TIMEOUT, executor)
            .await
            .context("fetching ACP registry")?;

    if status.is_client_error() {
        let text = String::from_utf8_lossy(body.as_slice());
        bail!(
            "registry status error {}, response: {text:?}",
            status.as_u16()
        );
    }

    let index: RegistryIndex = serde_json::from_slice(&body).context("parsing ACP registry")?;
    Ok(RegistryFetchResult {
        index,
        raw_body: body,
    })
}

async fn build_registry_agents(
    fs: Arc<dyn Fs>,
    http_client: Arc<dyn HttpClient>,
    index: RegistryIndex,
    raw_body: Vec<u8>,
    update_cache: bool,
    executor: &BackgroundExecutor,
) -> Result<Vec<RegistryAgent>> {
    let cache_dir = registry_cache_dir();
    fs.create_dir(&cache_dir).await?;

    let cache_path = cache_dir.join("registry.json");
    if update_cache {
        fs.write(&cache_path, &raw_body).await?;
    }

    let icons_dir = cache_dir.join("icons");
    if update_cache {
        fs.create_dir(&icons_dir).await?;
    }

    let current_platform = current_platform_key();
    let icon_paths = resolve_icon_paths(
        &index.agents,
        &icons_dir,
        update_cache,
        fs.clone(),
        http_client.clone(),
        executor,
    )
    .await;

    let mut agents = Vec::new();
    for (entry, icon_path) in index.agents.into_iter().zip(icon_paths) {
        let metadata = RegistryAgentMetadata {
            id: AgentId::new(entry.id),
            name: entry.name.into(),
            description: entry.description.into(),
            version: entry.version.into(),
            repository: entry.repository.map(Into::into),
            website: entry.website.map(Into::into),
            icon_path,
        };

        let binary_agent = entry.distribution.binary.as_ref().and_then(|binary| {
            if binary.is_empty() {
                return None;
            }

            let mut targets = HashMap::default();
            for (platform, target) in binary.iter() {
                targets.insert(
                    platform.clone(),
                    RegistryTargetConfig {
                        archive: target.archive.clone(),
                        cmd: target.cmd.clone(),
                        args: target.args.clone(),
                        sha256: target.sha256.clone(),
                        env: target.env.clone(),
                    },
                );
            }

            let supports_current_platform = current_platform
                .as_ref()
                .is_some_and(|platform| targets.contains_key(*platform));

            Some(RegistryBinaryAgent {
                metadata: metadata.clone(),
                targets,
                supports_current_platform,
            })
        });

        let npx_agent = entry.distribution.npx.as_ref().map(|npx| RegistryNpxAgent {
            metadata: metadata.clone(),
            package: npx.package.clone().into(),
            args: npx.args.clone(),
            env: npx.env.clone(),
        });

        let agent = match (binary_agent, npx_agent) {
            (Some(binary_agent), Some(npx_agent)) => {
                if binary_agent.supports_current_platform {
                    RegistryAgent::Binary(binary_agent)
                } else {
                    RegistryAgent::Npx(npx_agent)
                }
            }
            (Some(binary_agent), None) => RegistryAgent::Binary(binary_agent),
            (None, Some(npx_agent)) => RegistryAgent::Npx(npx_agent),
            (None, None) => continue,
        };

        agents.push(agent);
    }

    Ok(agents)
}

async fn resolve_icon_paths(
    entries: &[RegistryEntry],
    icons_dir: &Path,
    update_cache: bool,
    fs: Arc<dyn Fs>,
    http_client: Arc<dyn HttpClient>,
    executor: &BackgroundExecutor,
) -> Vec<Option<SharedString>> {
    join_all(entries.iter().map(|entry| {
        let fs = fs.clone();
        let http_client = http_client.clone();
        async move {
            resolve_icon_path(entry, icons_dir, update_cache, fs, http_client, executor)
                .await
                .log_err()
                .flatten()
        }
    }))
    .await
}

async fn resolve_icon_path(
    entry: &RegistryEntry,
    icons_dir: &Path,
    update_cache: bool,
    fs: Arc<dyn Fs>,
    http_client: Arc<dyn HttpClient>,
    executor: &BackgroundExecutor,
) -> Result<Option<SharedString>> {
    let icon_url = resolve_icon_url(entry);
    let Some(icon_url) = icon_url else {
        return Ok(None);
    };

    let icon_path = icons_dir.join(format!("{}.svg", entry.id));
    if update_cache && !fs.is_file(&icon_path).await {
        if let Err(error) = download_icon(fs.clone(), http_client, &icon_url, entry, executor).await
        {
            log::warn!(
                "Failed to download ACP registry icon for {}: {error:#}",
                entry.id
            );
        }
    }

    if fs.is_file(&icon_path).await {
        Ok(Some(SharedString::from(
            icon_path.to_string_lossy().into_owned(),
        )))
    } else {
        Ok(None)
    }
}

async fn download_icon(
    fs: Arc<dyn Fs>,
    http_client: Arc<dyn HttpClient>,
    icon_url: &str,
    entry: &RegistryEntry,
    executor: &BackgroundExecutor,
) -> Result<()> {
    let (status, body) =
        fetch_url_body(http_client, icon_url, REGISTRY_ICON_FETCH_TIMEOUT, executor)
            .await
            .with_context(|| format!("fetching icon for {}", entry.id))?;

    if status.is_client_error() {
        let text = String::from_utf8_lossy(body.as_slice());
        bail!("icon status error {}, response: {text:?}", status.as_u16());
    }

    let icon_path = registry_cache_dir()
        .join("icons")
        .join(format!("{}.svg", entry.id));
    fs.write(&icon_path, &body).await?;
    Ok(())
}

async fn fetch_url_body(
    http_client: Arc<dyn HttpClient>,
    url: &str,
    timeout: Duration,
    executor: &BackgroundExecutor,
) -> Result<(StatusCode, Vec<u8>)> {
    async {
        let mut response = http_client
            .get(url, AsyncBody::default(), true)
            .await
            .with_context(|| format!("requesting {url}"))?;

        let status = response.status();
        let mut body = Vec::new();
        response
            .body_mut()
            .read_to_end(&mut body)
            .await
            .with_context(|| format!("reading response from {url}"))?;

        Ok((status, body))
    }
    .with_timeout(timeout, executor)
    .await
    .map_err(|_| {
        anyhow!(
            "timed out after {}s while fetching {url}",
            timeout.as_secs()
        )
    })?
}

fn resolve_icon_url(entry: &RegistryEntry) -> Option<String> {
    let icon = entry.icon.as_ref()?;
    if icon.starts_with("https://") || icon.starts_with("http://") {
        return Some(icon.to_string());
    }

    let relative_icon = icon.trim_start_matches("./");
    Some(format!(
        "https://raw.githubusercontent.com/agentclientprotocol/registry/main/{}/{relative_icon}",
        entry.id
    ))
}

fn current_platform_key() -> Option<&'static str> {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        return None;
    };

    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else {
        return None;
    };

    Some(match os {
        "darwin" => match arch {
            "aarch64" => "darwin-aarch64",
            "x86_64" => "darwin-x86_64",
            _ => return None,
        },
        "linux" => match arch {
            "aarch64" => "linux-aarch64",
            "x86_64" => "linux-x86_64",
            _ => return None,
        },
        "windows" => match arch {
            "aarch64" => "windows-aarch64",
            "x86_64" => "windows-x86_64",
            _ => return None,
        },
        _ => return None,
    })
}

fn registry_cache_dir() -> PathBuf {
    paths::external_agents_dir().join("registry")
}

fn registry_cache_path() -> PathBuf {
    registry_cache_dir().join("registry.json")
}

#[derive(Deserialize)]
struct RegistryIndex {
    #[serde(rename = "version")]
    _version: String,
    agents: Vec<RegistryEntry>,
}

#[derive(Deserialize)]
struct RegistryEntry {
    id: String,
    name: String,
    version: String,
    description: String,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    website: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    distribution: RegistryDistribution,
}

#[derive(Deserialize)]
struct RegistryDistribution {
    #[serde(default)]
    binary: Option<HashMap<String, RegistryBinaryTarget>>,
    #[serde(default)]
    npx: Option<RegistryNpxDistribution>,
}

#[derive(Deserialize)]
struct RegistryBinaryTarget {
    archive: String,
    cmd: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    env: HashMap<String, String>,
}

#[derive(Deserialize)]
struct RegistryNpxDistribution {
    package: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn npx_agent(id: &str, version: &str, package: &str) -> RegistryAgent {
        npx_agent_with_args(id, version, package, &[])
    }

    fn npx_agent_with_args(id: &str, version: &str, package: &str, args: &[&str]) -> RegistryAgent {
        RegistryAgent::Npx(RegistryNpxAgent {
            metadata: RegistryAgentMetadata {
                id: AgentId::new(id.to_string()),
                name: id.to_string().into(),
                description: SharedString::default(),
                version: version.to_string().into(),
                repository: None,
                website: None,
                icon_path: None,
            },
            package: package.to_string().into(),
            args: args.iter().map(|argument| argument.to_string()).collect(),
            env: HashMap::default(),
        })
    }

    fn merge_remote_orion_code(agent: RegistryAgent) -> RegistryAgent {
        merge_registry_agents(first_party_registry_agents(None), vec![agent])
            .into_iter()
            .find(|agent| agent.id().as_ref() == ORION_CODE_AGENT_ID)
            .expect("merged Registry should contain Orion Code")
    }

    #[test]
    fn first_party_orion_code_descriptor_is_available_without_remote_registry() {
        let merged = merge_registry_agents(first_party_registry_agents(None), Vec::new());
        let orion_code = merged
            .iter()
            .find(|agent| agent.id().as_ref() == ORION_CODE_AGENT_ID)
            .expect("first-party Orion Code descriptor should exist");

        assert_eq!(orion_code.name().as_ref(), "Orion Code");
        assert!(!orion_code.is_installable());
        assert!(
            orion_code
                .unavailable_reason()
                .is_some_and(|reason| reason.contains("ACP Registry"))
        );
    }

    #[test]
    fn remote_orion_code_npx_distribution_replaces_unavailable_descriptor_once() {
        let merged = merge_registry_agents(
            first_party_registry_agents(None),
            vec![npx_agent(
                ORION_CODE_AGENT_ID,
                "0.3.2",
                "@orion-agents/orion-code@0.3.2",
            )],
        );
        let matching = merged
            .iter()
            .filter(|agent| agent.id().as_ref() == ORION_CODE_AGENT_ID)
            .collect::<Vec<_>>();

        assert_eq!(matching.len(), 1, "descriptor merge must not duplicate IDs");
        let RegistryAgent::Npx(orion_code) = matching[0] else {
            panic!("remote npx distribution should replace the unavailable descriptor");
        };
        assert_eq!(
            orion_code.package.as_ref(),
            "@orion-agents/orion-code@0.3.2"
        );
        assert_eq!(orion_code.metadata.name.as_ref(), "Orion Code");
        assert!(orion_code.metadata.repository.is_none());
        assert!(orion_code.metadata.website.is_none());
        assert!(orion_code.metadata.icon_path.is_none());
        assert!(
            orion_code.args.is_empty(),
            "the dedicated orion-code bin requires Registry npx args=[]"
        );
    }

    #[test]
    fn remote_orion_code_accepts_exact_stable_and_prerelease_packages() {
        for version in ["1.2.3", "1.2.3-rc.1", "1.2.3-rc.1+build.5"] {
            let package = format!("{ORION_CODE_NPM_PACKAGE}@{version}");
            let agent =
                merge_remote_orion_code(npx_agent(ORION_CODE_AGENT_ID, version, package.as_str()));

            let RegistryAgent::Npx(agent) = agent else {
                panic!("exact Orion Code package {package} should remain installable");
            };
            assert_eq!(agent.package.as_ref(), package.as_str());
            assert!(agent.args.is_empty());
        }
    }

    #[test]
    fn remote_orion_code_rejects_untrusted_or_inexact_npx_descriptors() {
        let cases = [
            (
                "wrong package name",
                "1.2.3",
                "@attacker/orion-code@1.2.3",
                Vec::new(),
                "trusted package",
            ),
            (
                "missing package version",
                "1.2.3",
                ORION_CODE_NPM_PACKAGE,
                Vec::new(),
                "include an exact semantic version",
            ),
            (
                "latest tag",
                "1.2.3",
                "@orion-agents/orion-code@latest",
                Vec::new(),
                "package version must be an exact semantic version",
            ),
            (
                "semver range",
                "1.2.3",
                "@orion-agents/orion-code@^1.2.3",
                Vec::new(),
                "package version must be an exact semantic version",
            ),
            (
                "git spec",
                "1.2.3",
                "git+https://example.invalid/orion-code.git",
                Vec::new(),
                "trusted package",
            ),
            (
                "file spec",
                "1.2.3",
                "file:../orion-code",
                Vec::new(),
                "trusted package",
            ),
            (
                "URL spec",
                "1.2.3",
                "https://example.invalid/orion-code.tgz",
                Vec::new(),
                "trusted package",
            ),
            (
                "metadata and package version mismatch",
                "1.2.3",
                "@orion-agents/orion-code@1.2.4",
                Vec::new(),
                "must exactly match metadata version",
            ),
            (
                "launcher arguments",
                "1.2.3",
                "@orion-agents/orion-code@1.2.3",
                vec!["--unsafe"],
                "must not declare launcher arguments",
            ),
        ];

        for (case, metadata_version, package, args, expected_reason) in cases {
            let agent = merge_remote_orion_code(npx_agent_with_args(
                ORION_CODE_AGENT_ID,
                metadata_version,
                package,
                args.as_slice(),
            ));

            let RegistryAgent::Unavailable(agent) = agent else {
                panic!("{case} must make Orion Code unavailable");
            };
            assert!(
                agent.reason.contains(expected_reason),
                "{case} should expose a useful reason, received: {}",
                agent.reason
            );
        }
    }

    #[test]
    fn remote_orion_code_rejects_non_semver_metadata_version() {
        let agent = merge_remote_orion_code(npx_agent(
            ORION_CODE_AGENT_ID,
            "latest",
            "@orion-agents/orion-code@1.2.3",
        ));

        let RegistryAgent::Unavailable(agent) = agent else {
            panic!("non-semver Orion Code metadata must be unavailable");
        };
        assert!(
            agent
                .reason
                .contains("metadata version must be an exact semantic version")
        );
    }

    #[test]
    fn remote_orion_code_rejects_distribution_environment_overrides() {
        let mut remote_agent = npx_agent(
            ORION_CODE_AGENT_ID,
            "1.2.3",
            "@orion-agents/orion-code@1.2.3",
        );
        let RegistryAgent::Npx(agent) = &mut remote_agent else {
            unreachable!("test fixture is an npx agent");
        };
        agent.env.insert(
            "NODE_OPTIONS".to_string(),
            "--require attacker.js".to_string(),
        );

        let RegistryAgent::Unavailable(agent) = merge_remote_orion_code(remote_agent) else {
            panic!("distribution environment must make Orion Code unavailable");
        };
        assert!(agent.reason.contains("environment variables"));
        assert_eq!(agent.metadata.name.as_ref(), "Orion Code");
        assert!(agent.metadata.version.is_empty());
    }

    #[test]
    fn remote_orion_code_rejects_binary_distribution_for_the_reserved_id() {
        let agent = merge_remote_orion_code(RegistryAgent::Binary(RegistryBinaryAgent {
            metadata: RegistryAgentMetadata {
                id: AgentId::new(ORION_CODE_AGENT_ID),
                name: "Untrusted Orion Code".into(),
                description: SharedString::default(),
                version: "9.9.9".into(),
                repository: None,
                website: None,
                icon_path: None,
            },
            targets: HashMap::default(),
            supports_current_platform: true,
        }));

        let RegistryAgent::Unavailable(agent) = agent else {
            panic!("remote binary distribution must not claim the reserved Orion Code ID");
        };
        assert!(agent.reason.contains("trusted exact-version npx"));
    }

    #[test]
    fn package_binding_does_not_change_other_remote_agents() {
        let merged = merge_registry_agents(
            first_party_registry_agents(None),
            vec![npx_agent("third-party-agent", "1.2.3", "agent@latest")],
        );
        let third_party = merged
            .iter()
            .find(|agent| agent.id().as_ref() == "third-party-agent")
            .expect("third-party agent should remain in the Registry");

        let RegistryAgent::Npx(third_party) = third_party else {
            panic!("Orion Code package binding must not apply to other agents");
        };
        assert_eq!(third_party.package.as_ref(), "agent@latest");
    }

    #[test]
    fn first_party_distribution_wins_equal_or_older_remote_versions() {
        let first_party = npx_agent(
            ORION_CODE_AGENT_ID,
            "1.2.0",
            "@orion-agents/orion-code@1.2.0",
        );
        let merged = merge_registry_agents(
            vec![first_party],
            vec![
                npx_agent(
                    ORION_CODE_AGENT_ID,
                    "1.2.0",
                    "unexpected-equal-version-package",
                ),
                npx_agent(ORION_CODE_AGENT_ID, "1.1.9", "unexpected-older-package"),
            ],
        );

        let RegistryAgent::Npx(orion_code) = &merged[0] else {
            panic!("first-party npx descriptor should remain selected");
        };
        assert_eq!(
            orion_code.package.as_ref(),
            "@orion-agents/orion-code@1.2.0"
        );
    }

    #[test]
    fn newer_remote_distribution_replaces_first_party_version() {
        let merged = merge_registry_agents(
            vec![npx_agent(
                ORION_CODE_AGENT_ID,
                "1.2.0",
                "@orion-agents/orion-code@1.2.0",
            )],
            vec![npx_agent(
                ORION_CODE_AGENT_ID,
                "1.3.0",
                "@orion-agents/orion-code@1.3.0",
            )],
        );

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].version().as_ref(), "1.3.0");
    }

    #[test]
    fn orion_code_verified_version_floor_rejects_remote_downgrade_but_allows_upgrade() {
        let floor = semver::Version::parse("2.0.0").expect("valid version floor");
        let downgraded = merge_registry_agents(
            first_party_registry_agents(Some(&floor)),
            vec![npx_agent(
                ORION_CODE_AGENT_ID,
                "1.9.9",
                "@orion-agents/orion-code@1.9.9",
            )],
        );
        assert_eq!(downgraded[0].version().as_ref(), "2.0.0");
        let RegistryAgent::Npx(downgraded) = &downgraded[0] else {
            panic!("verified floor should remain an exact npx distribution");
        };
        assert_eq!(
            downgraded.package.as_ref(),
            "@orion-agents/orion-code@2.0.0"
        );

        let upgraded = merge_registry_agents(
            first_party_registry_agents(Some(&floor)),
            vec![npx_agent(
                ORION_CODE_AGENT_ID,
                "2.1.0",
                "@orion-agents/orion-code@2.1.0",
            )],
        );
        assert_eq!(upgraded[0].version().as_ref(), "2.1.0");
    }

    #[test]
    fn orion_code_rollback_pin_restores_the_verified_version_over_a_newer_candidate() {
        let mut agents = vec![npx_agent(
            ORION_CODE_AGENT_ID,
            "2.1.0",
            "@orion-agents/orion-code@2.1.0",
        )];
        let verified = semver::Version::parse("2.0.0").expect("valid verified version");

        apply_orion_code_version_pin(&mut agents, Some(&verified));

        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].version().as_ref(), "2.0.0");
        let RegistryAgent::Npx(agent) = &agents[0] else {
            panic!("rollback pin should use the trusted npx descriptor");
        };
        assert_eq!(agent.package.as_ref(), "@orion-agents/orion-code@2.0.0");
    }
}
