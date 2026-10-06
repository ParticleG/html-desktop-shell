use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
};

use webkit6::{
    LoadEvent, NavigationPolicyDecision, PolicyDecisionType, ResponsePolicyDecision, Settings,
    WebContext, WebView, gio, glib, prelude::*,
};

use crate::assets::ResolvedWebAssets;

const TRUSTED_SCHEME: &str = "html-desktop-shell";
const TRUSTED_AUTHORITY: &str = "assets";
pub const TRUSTED_ENTRY_URI: &str = "html-desktop-shell://assets/index.html";
const TRUSTED_ORIGIN: &str = "html-desktop-shell://assets";
const RESPONSE_SECURITY_HEADERS: &[(&str, &str)] = &[
    ("Access-Control-Allow-Origin", TRUSTED_ORIGIN),
    ("Vary", "Origin"),
    ("Cross-Origin-Resource-Policy", "same-origin"),
];

const DEFAULT_CSP: &str = concat!(
    "default-src 'none'; ",
    "script-src 'self' 'unsafe-inline'; ",
    "style-src 'self' 'unsafe-inline'; ",
    "img-src 'self' data:; ",
    "font-src 'self'; ",
    "media-src 'self'; ",
    "connect-src 'self'; ",
    "object-src 'none'; ",
    "frame-src 'none'; ",
    "child-src 'none'; ",
    "worker-src 'none'; ",
    "form-action 'none'; ",
    "base-uri 'none'; ",
    "frame-ancestors 'none'",
);

#[derive(Clone)]
pub struct TrustedWebContext {
    context: WebContext,
}

impl TrustedWebContext {
    pub fn new(assets: ResolvedWebAssets) -> Result<Self, String> {
        let context = WebContext::new();
        let security_manager = context
            .security_manager()
            .ok_or_else(|| "WebKit did not provide a security manager".to_owned())?;

        security_manager.register_uri_scheme_as_local(TRUSTED_SCHEME);
        security_manager.register_uri_scheme_as_secure(TRUSTED_SCHEME);
        security_manager.register_uri_scheme_as_cors_enabled(TRUSTED_SCHEME);
        security_manager.register_uri_scheme_as_display_isolated(TRUSTED_SCHEME);

        context.register_uri_scheme(TRUSTED_SCHEME, move |request| {
            let result = request
                .uri()
                .ok_or_else(|| SchemeError::Invalid("request has no URI".to_owned()))
                .and_then(|uri| resolve_asset_request(assets.root(), uri.as_str()))
                .and_then(|asset| {
                    let length = i64::try_from(asset.length).map_err(|_| {
                        SchemeError::Io(format!(
                            "asset {} is too large to serve",
                            asset.canonical.display()
                        ))
                    })?;
                    let stream = gio::File::for_path(&asset.canonical)
                        .read(None::<&gio::Cancellable>)
                        .map_err(|error| {
                            SchemeError::Io(format!(
                                "failed to open {}: {error}",
                                asset.canonical.display()
                            ))
                        })?;
                    let mime_type = mime_type_for_logical_path(&asset.logical);
                    Ok((stream, length, mime_type))
                });

            match result {
                Ok((stream, length, mime_type)) => {
                    let response = webkit6::URISchemeResponse::new(&stream, length);
                    response.set_status(200, Some("OK"));
                    response.set_content_type(mime_type.as_str());
                    let headers = webkit6::soup::MessageHeaders::new(
                        webkit6::soup::MessageHeadersType::Response,
                    );
                    for (name, value) in RESPONSE_SECURITY_HEADERS {
                        headers.append(name, value);
                    }
                    response.set_http_headers(headers);
                    request.finish_with_response(&response);
                }
                Err(error) => {
                    let mut error = glib::Error::new(error.io_kind(), &error.to_string());
                    request.finish_error(&mut error);
                }
            }
        });

        Ok(Self { context })
    }

    pub fn secure_web_view(&self) -> (WebView, WebViewTrust) {
        let settings = Settings::new();
        settings.set_allow_file_access_from_file_urls(false);
        settings.set_allow_universal_access_from_file_urls(false);

        let web_view = WebView::builder()
            .web_context(&self.context)
            .settings(&settings)
            .default_content_security_policy(DEFAULT_CSP)
            .build();
        let trust = WebViewTrust::new();
        install_security_hooks(&web_view, &trust);
        (web_view, trust)
    }
}

#[derive(Clone)]
pub struct WebViewTrust {
    state: Rc<Cell<TrustState>>,
}

impl WebViewTrust {
    fn new() -> Self {
        Self {
            state: Rc::new(Cell::new(TrustState::Armed)),
        }
    }

    pub fn allows_bridge(&self, web_view: &WebView) -> bool {
        bridge_allowed(
            self.state.get(),
            web_view.uri().as_ref().map(|uri| uri.as_str()),
        )
    }

    fn revoke(&self) {
        self.state.set(TrustState::Revoked);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrustState {
    Armed,
    Loading,
    Trusted,
    Revoked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NavigationKind {
    Direct,
    Redirect,
    Popup,
}

fn reduce_navigation(
    state: TrustState,
    kind: NavigationKind,
    uri: Option<&str>,
) -> (TrustState, bool) {
    match (state, kind, uri) {
        (TrustState::Armed, NavigationKind::Direct, Some(TRUSTED_ENTRY_URI)) => {
            (TrustState::Loading, true)
        }
        _ => (TrustState::Revoked, false),
    }
}

fn reduce_load_event(state: TrustState, event: LoadEvent, uri: Option<&str>) -> TrustState {
    match event {
        LoadEvent::Started
            if matches!(state, TrustState::Armed | TrustState::Loading)
                && uri == Some(TRUSTED_ENTRY_URI) =>
        {
            state
        }
        LoadEvent::Committed if state == TrustState::Loading && uri == Some(TRUSTED_ENTRY_URI) => {
            TrustState::Trusted
        }
        LoadEvent::Finished if state == TrustState::Trusted && uri == Some(TRUSTED_ENTRY_URI) => {
            state
        }
        LoadEvent::Redirected | LoadEvent::Started | LoadEvent::Committed | LoadEvent::Finished => {
            TrustState::Revoked
        }
        _ => TrustState::Revoked,
    }
}

fn reduce_uri_observation(state: TrustState, uri: Option<&str>) -> TrustState {
    if state == TrustState::Trusted && uri != Some(TRUSTED_ENTRY_URI) {
        TrustState::Revoked
    } else {
        state
    }
}

fn bridge_allowed(state: TrustState, uri: Option<&str>) -> bool {
    state == TrustState::Trusted && uri == Some(TRUSTED_ENTRY_URI)
}

fn install_security_hooks(web_view: &WebView, trust: &WebViewTrust) {
    let policy_trust = trust.clone();
    web_view.connect_decide_policy(move |_, decision, decision_type| match decision_type {
        PolicyDecisionType::NavigationAction => {
            let Some(navigation) = decision.downcast_ref::<NavigationPolicyDecision>() else {
                policy_trust.revoke();
                decision.ignore();
                return true;
            };
            let Some(action) = navigation.navigation_action() else {
                policy_trust.revoke();
                decision.ignore();
                return true;
            };
            let uri = action.request().and_then(|request| request.uri());
            let kind = if action.is_redirect() {
                NavigationKind::Redirect
            } else {
                NavigationKind::Direct
            };
            let (next, allow) = reduce_navigation(
                policy_trust.state.get(),
                kind,
                uri.as_ref().map(|uri| uri.as_str()),
            );
            policy_trust.state.set(next);
            if allow {
                decision.use_();
            } else {
                decision.ignore();
            }
            true
        }
        PolicyDecisionType::NewWindowAction => {
            let (next, _) =
                reduce_navigation(policy_trust.state.get(), NavigationKind::Popup, None);
            policy_trust.state.set(next);
            decision.ignore();
            true
        }
        PolicyDecisionType::Response => {
            let Some(response) = decision.downcast_ref::<ResponsePolicyDecision>() else {
                policy_trust.revoke();
                decision.ignore();
                return true;
            };
            let uri = response.request().and_then(|request| request.uri());
            let uri = uri.as_ref().map(|uri| uri.as_str());
            let allow = if response.is_main_frame_main_resource() {
                matches!(
                    policy_trust.state.get(),
                    TrustState::Loading | TrustState::Trusted
                ) && uri == Some(TRUSTED_ENTRY_URI)
            } else {
                policy_trust.state.get() != TrustState::Revoked
                    && uri.is_some_and(is_trusted_origin_uri)
            };
            if allow {
                decision.use_();
            } else {
                policy_trust.revoke();
                decision.ignore();
            }
            true
        }
        _ => {
            policy_trust.revoke();
            decision.ignore();
            true
        }
    });

    let create_trust = trust.clone();
    web_view.connect_create(move |_, _| {
        let (next, _) = reduce_navigation(create_trust.state.get(), NavigationKind::Popup, None);
        create_trust.state.set(next);
        None
    });

    let load_trust = trust.clone();
    web_view.connect_load_changed(move |web_view, event| {
        let uri = web_view.uri();
        let next = reduce_load_event(
            load_trust.state.get(),
            event,
            uri.as_ref().map(|uri| uri.as_str()),
        );
        load_trust.state.set(next);
    });

    let uri_trust = trust.clone();
    web_view.connect_uri_notify(move |web_view| {
        let uri = web_view.uri();
        let next =
            reduce_uri_observation(uri_trust.state.get(), uri.as_ref().map(|uri| uri.as_str()));
        uri_trust.state.set(next);
    });

    let failure_trust = trust.clone();
    web_view.connect_load_failed(move |_, _, _, _| {
        failure_trust.revoke();
        false
    });

    let resource_trust = trust.clone();
    web_view.connect_resource_load_started(move |_, _, request| {
        let trusted = request
            .uri()
            .as_ref()
            .is_some_and(|uri| is_trusted_origin_uri(uri.as_str()));
        if !trusted {
            resource_trust.revoke();
        }
    });
}

fn is_trusted_origin_uri(uri: &str) -> bool {
    parse_custom_uri(uri).is_ok()
}

#[derive(Debug)]
enum SchemeError {
    Invalid(String),
    NotFound(String),
    Io(String),
}

impl SchemeError {
    fn io_kind(&self) -> gio::IOErrorEnum {
        match self {
            Self::Invalid(_) => gio::IOErrorEnum::InvalidArgument,
            Self::NotFound(_) => gio::IOErrorEnum::NotFound,
            Self::Io(_) => gio::IOErrorEnum::Failed,
        }
    }
}

impl std::fmt::Display for SchemeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) | Self::NotFound(message) | Self::Io(message) => {
                formatter.write_str(message)
            }
        }
    }
}

fn parse_custom_uri(uri: &str) -> Result<String, SchemeError> {
    let rest = uri
        .strip_prefix(TRUSTED_SCHEME)
        .and_then(|rest| rest.strip_prefix("://"))
        .ok_or_else(|| SchemeError::Invalid("invalid URI scheme or shape".to_owned()))?;
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority != TRUSTED_AUTHORITY || authority.contains('@') || authority.contains(':') {
        return Err(SchemeError::Invalid("invalid URI authority".to_owned()));
    }

    let path_and_suffix = &rest[authority_end..];
    let path_end = path_and_suffix
        .find(['?', '#'])
        .unwrap_or(path_and_suffix.len());
    let path = &path_and_suffix[..path_end];
    if !path.starts_with('/') {
        return Err(SchemeError::Invalid("URI path must be absolute".to_owned()));
    }
    percent_decode_path(path)
}

fn percent_decode_path(path: &str) -> Result<String, SchemeError> {
    let input = path.as_bytes();
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index] == b'%' {
            if index + 2 >= input.len() {
                return Err(SchemeError::Invalid("malformed percent escape".to_owned()));
            }
            let high = decode_hex(input[index + 1])
                .ok_or_else(|| SchemeError::Invalid("malformed percent escape".to_owned()))?;
            let low = decode_hex(input[index + 2])
                .ok_or_else(|| SchemeError::Invalid("malformed percent escape".to_owned()))?;
            output.push((high << 4) | low);
            index += 3;
        } else {
            output.push(input[index]);
            index += 1;
        }
    }

    let decoded = String::from_utf8(output)
        .map_err(|_| SchemeError::Invalid("URI path is not UTF-8".to_owned()))?;
    if decoded.contains(['\\', '\0']) {
        return Err(SchemeError::Invalid(
            "URI path contains a forbidden character".to_owned(),
        ));
    }
    if decoded
        .split('/')
        .skip(1)
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(SchemeError::Invalid(
            "URI path is not normalized".to_owned(),
        ));
    }
    Ok(decoded)
}

fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

struct ResolvedAsset {
    canonical: PathBuf,
    logical: PathBuf,
    length: u64,
}

fn resolve_asset_request(root: &Path, uri: &str) -> Result<ResolvedAsset, SchemeError> {
    let decoded_path = parse_custom_uri(uri)?;
    let relative = decoded_path
        .strip_prefix('/')
        .ok_or_else(|| SchemeError::Invalid("URI path must be absolute".to_owned()))?;
    let logical = PathBuf::from(relative);
    let candidate = root.join(&logical);
    let canonical = candidate.canonicalize().map_err(|error| {
        SchemeError::NotFound(format!(
            "asset {} is unavailable: {error}",
            candidate.display()
        ))
    })?;
    if !canonical.starts_with(root) {
        return Err(SchemeError::Invalid(
            "asset resolves outside the asset root".to_owned(),
        ));
    }
    let metadata = canonical.metadata().map_err(|error| {
        SchemeError::NotFound(format!(
            "asset {} is unavailable: {error}",
            canonical.display()
        ))
    })?;
    if !metadata.file_type().is_file() {
        return Err(SchemeError::Invalid(
            "asset is not a regular file".to_owned(),
        ));
    }
    Ok(ResolvedAsset {
        canonical,
        logical,
        length: metadata.len(),
    })
}

fn mime_type_for_logical_path(logical_path: &Path) -> glib::GString {
    let extension = logical_path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    let known = match extension {
        value if value.eq_ignore_ascii_case("html") => Some("text/html"),
        value if value.eq_ignore_ascii_case("css") => Some("text/css"),
        value if value.eq_ignore_ascii_case("js") || value.eq_ignore_ascii_case("mjs") => {
            Some("text/javascript")
        }
        value if value.eq_ignore_ascii_case("json") => Some("application/json"),
        value if value.eq_ignore_ascii_case("svg") => Some("image/svg+xml"),
        value if value.eq_ignore_ascii_case("png") => Some("image/png"),
        value if value.eq_ignore_ascii_case("jpg") || value.eq_ignore_ascii_case("jpeg") => {
            Some("image/jpeg")
        }
        value if value.eq_ignore_ascii_case("gif") => Some("image/gif"),
        value if value.eq_ignore_ascii_case("webp") => Some("image/webp"),
        value if value.eq_ignore_ascii_case("ico") => Some("image/x-icon"),
        value if value.eq_ignore_ascii_case("woff") => Some("font/woff"),
        value if value.eq_ignore_ascii_case("woff2") => Some("font/woff2"),
        value if value.eq_ignore_ascii_case("ttf") => Some("font/ttf"),
        value if value.eq_ignore_ascii_case("otf") => Some("font/otf"),
        value if value.eq_ignore_ascii_case("wasm") => Some("application/wasm"),
        value if value.eq_ignore_ascii_case("xml") => Some("application/xml"),
        value if value.eq_ignore_ascii_case("txt") => Some("text/plain"),
        value if value.eq_ignore_ascii_case("mp3") => Some("audio/mpeg"),
        value if value.eq_ignore_ascii_case("ogg") => Some("audio/ogg"),
        value if value.eq_ignore_ascii_case("wav") => Some("audio/wav"),
        value if value.eq_ignore_ascii_case("mp4") => Some("video/mp4"),
        value if value.eq_ignore_ascii_case("webm") => Some("video/webm"),
        _ => None,
    };
    if let Some(mime_type) = known {
        return mime_type.into();
    }
    let (content_type, _) = gio::content_type_guess(Some(logical_path), None::<&[u8]>);
    gio::content_type_get_mime_type(&content_type)
        .unwrap_or_else(|| "application/octet-stream".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn csp_allows_local_inline_code_without_remote_or_eval_sources() {
        assert!(DEFAULT_CSP.contains("script-src 'self' 'unsafe-inline'"));
        assert!(DEFAULT_CSP.contains("style-src 'self' 'unsafe-inline'"));
        for forbidden in ["'unsafe-eval'", "http:", "https:", "ws:", "wss:", "file:"] {
            assert!(!DEFAULT_CSP.contains(forbidden), "CSP contains {forbidden}");
        }
    }

    #[test]
    fn custom_scheme_response_allows_cors_only_for_the_trusted_origin() {
        assert_eq!(
            RESPONSE_SECURITY_HEADERS,
            &[
                ("Access-Control-Allow-Origin", "html-desktop-shell://assets"),
                ("Vary", "Origin"),
                ("Cross-Origin-Resource-Policy", "same-origin"),
            ]
        );
        assert!(
            !RESPONSE_SECURITY_HEADERS
                .iter()
                .any(|(_, value)| *value == "*"
                    || value.contains("http://")
                    || value.contains("https://"))
        );
    }

    #[test]
    fn custom_uri_requires_official_shape_and_authority() {
        assert_eq!(parse_custom_uri(TRUSTED_ENTRY_URI).unwrap(), "/index.html");
        for uri in [
            "html-desktop-shell:index.html",
            "html-desktop-shell:///index.html",
            "html-desktop-shell://user@assets/index.html",
            "html-desktop-shell://assets:80/index.html",
            "html-desktop-shell://other/index.html",
            "file:///index.html",
        ] {
            assert!(parse_custom_uri(uri).is_err(), "accepted {uri}");
        }
    }

    #[test]
    fn path_validation_decodes_once_and_rejects_observable_escape_shapes() {
        for uri in [
            "html-desktop-shell://assets/../secret",
            "html-desktop-shell://assets/%2e%2e/secret",
            "html-desktop-shell://assets/a%2fb/../secret",
            "html-desktop-shell://assets/a%5cb",
            "html-desktop-shell://assets/a%00b",
            "html-desktop-shell://assets//index.html",
            "html-desktop-shell://assets/%ZZ",
        ] {
            assert!(parse_custom_uri(uri).is_err(), "accepted {uri}");
        }
        assert_eq!(
            parse_custom_uri("html-desktop-shell://assets/icons%20and%20fonts/app.svg").unwrap(),
            "/icons and fonts/app.svg"
        );
    }

    #[test]
    fn only_exact_entry_can_consume_initial_navigation() {
        let cases = [
            (Some(TRUSTED_ENTRY_URI), true),
            (Some("html-desktop-shell://assets/sibling.html"), false),
            (Some("html-desktop-shell://assets/index.html?x=1"), false),
            (
                Some("html-desktop-shell://assets/index.html#fragment"),
                false,
            ),
            (Some("https://example.com/"), false),
            (Some("html-desktop-shell://assets/../index.html"), false),
            (None, false),
        ];
        for (uri, expected) in cases {
            let (state, allowed) =
                reduce_navigation(TrustState::Armed, NavigationKind::Direct, uri);
            assert_eq!(allowed, expected, "URI {uri:?}");
            assert_eq!(
                state,
                if expected {
                    TrustState::Loading
                } else {
                    TrustState::Revoked
                }
            );
        }
    }

    #[test]
    fn redirect_popup_and_every_later_navigation_revoke() {
        for kind in [NavigationKind::Redirect, NavigationKind::Popup] {
            let (state, allowed) =
                reduce_navigation(TrustState::Armed, kind, Some(TRUSTED_ENTRY_URI));
            assert_eq!((state, allowed), (TrustState::Revoked, false));
        }
        for state in [
            TrustState::Loading,
            TrustState::Trusted,
            TrustState::Revoked,
        ] {
            let result = reduce_navigation(state, NavigationKind::Direct, Some(TRUSTED_ENTRY_URI));
            assert_eq!(result, (TrustState::Revoked, false));
        }
    }

    #[test]
    fn same_origin_subresources_are_recognized_without_admitting_remote_origins() {
        for uri in [
            "html-desktop-shell://assets/assets/app.js",
            "html-desktop-shell://assets/fonts/app.woff2?v=1",
            "html-desktop-shell://assets/image.svg#icon",
        ] {
            assert!(is_trusted_origin_uri(uri), "rejected {uri}");
        }
        for uri in [
            "https://assets/app.js",
            "http://example.com/app.js",
            "file:///tmp/app.js",
            "html-desktop-shell://other/app.js",
            "html-desktop-shell://assets.example/app.js",
            "html-desktop-shell://user@assets/app.js",
            "html-desktop-shell://assets:443/app.js",
        ] {
            assert!(!is_trusted_origin_uri(uri), "accepted {uri}");
        }
    }

    #[test]
    fn bridge_gate_requires_trusted_state_and_exact_current_uri() {
        for state in [TrustState::Armed, TrustState::Loading, TrustState::Revoked] {
            assert!(!bridge_allowed(state, Some(TRUSTED_ENTRY_URI)));
        }
        assert!(bridge_allowed(TrustState::Trusted, Some(TRUSTED_ENTRY_URI)));
        assert!(!bridge_allowed(
            TrustState::Trusted,
            Some("html-desktop-shell://assets/index.html?changed")
        ));
        assert!(!bridge_allowed(TrustState::Trusted, None));
    }

    #[test]
    fn history_uri_change_revokes_permanently_even_if_entry_is_restored() {
        let state = reduce_uri_observation(
            TrustState::Trusted,
            Some("html-desktop-shell://assets/changed"),
        );
        assert_eq!(state, TrustState::Revoked);
        assert_eq!(
            reduce_uri_observation(state, Some(TRUSTED_ENTRY_URI)),
            TrustState::Revoked
        );
        assert!(!bridge_allowed(state, Some(TRUSTED_ENTRY_URI)));
    }

    #[test]
    fn initial_load_allows_both_started_and_policy_signal_orders() {
        let started_first = reduce_load_event(
            TrustState::Armed,
            LoadEvent::Started,
            Some(TRUSTED_ENTRY_URI),
        );
        assert_eq!(started_first, TrustState::Armed);
        let (started_first, allowed) = reduce_navigation(
            started_first,
            NavigationKind::Direct,
            Some(TRUSTED_ENTRY_URI),
        );
        assert!(allowed);
        assert_eq!(
            reduce_load_event(started_first, LoadEvent::Committed, Some(TRUSTED_ENTRY_URI)),
            TrustState::Trusted
        );

        let (policy_first, allowed) = reduce_navigation(
            TrustState::Armed,
            NavigationKind::Direct,
            Some(TRUSTED_ENTRY_URI),
        );
        assert!(allowed);
        let policy_first =
            reduce_load_event(policy_first, LoadEvent::Started, Some(TRUSTED_ENTRY_URI));
        assert_eq!(policy_first, TrustState::Loading);
        assert_eq!(
            reduce_load_event(policy_first, LoadEvent::Committed, Some(TRUSTED_ENTRY_URI)),
            TrustState::Trusted
        );

        assert_eq!(
            reduce_load_event(
                TrustState::Armed,
                LoadEvent::Started,
                Some("html-desktop-shell://assets/other.html"),
            ),
            TrustState::Revoked
        );
    }

    #[test]
    fn commit_is_the_only_transition_to_trusted_and_mismatches_revoke() {
        assert_eq!(
            reduce_load_event(
                TrustState::Loading,
                LoadEvent::Committed,
                Some(TRUSTED_ENTRY_URI)
            ),
            TrustState::Trusted
        );
        assert_eq!(
            reduce_load_event(
                TrustState::Loading,
                LoadEvent::Committed,
                Some("html-desktop-shell://assets/other.html")
            ),
            TrustState::Revoked
        );
        assert_eq!(
            reduce_load_event(
                TrustState::Loading,
                LoadEvent::Redirected,
                Some(TRUSTED_ENTRY_URI)
            ),
            TrustState::Revoked
        );
    }

    #[test]
    fn explicit_web_mime_types_follow_the_logical_request_extension() {
        for (path, expected) in [
            ("index.html", "text/html"),
            ("app.css", "text/css"),
            ("app.js", "text/javascript"),
            ("chunk.mjs", "text/javascript"),
            ("data.json", "application/json"),
            ("icon.svg", "image/svg+xml"),
            ("image.png", "image/png"),
            ("image.jpg", "image/jpeg"),
            ("image.jpeg", "image/jpeg"),
            ("image.gif", "image/gif"),
            ("image.webp", "image/webp"),
            ("favicon.ico", "image/x-icon"),
            ("font.woff", "font/woff"),
            ("font.woff2", "font/woff2"),
            ("font.ttf", "font/ttf"),
            ("font.otf", "font/otf"),
            ("module.wasm", "application/wasm"),
            ("document.xml", "application/xml"),
            ("readme.txt", "text/plain"),
            ("audio.mp3", "audio/mpeg"),
            ("audio.ogg", "audio/ogg"),
            ("audio.wav", "audio/wav"),
            ("video.mp4", "video/mp4"),
            ("video.webm", "video/webm"),
        ] {
            assert_eq!(
                mime_type_for_logical_path(Path::new(path)),
                expected,
                "{path}"
            );
        }
        assert_eq!(
            mime_type_for_logical_path(Path::new("UPPER.JS")),
            "text/javascript"
        );
    }

    #[cfg(unix)]
    #[test]
    fn canonical_resolution_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("html-desktop-shell-web-security-{nonce}"));
        let root = base.join("root");
        let outside = base.join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(root.join("index.html"), b"safe").unwrap();
        fs::write(root.join("physical.css"), b"contained").unwrap();
        fs::write(outside.join("secret.js"), b"secret").unwrap();
        let canonical_root = root.canonicalize().unwrap();
        symlink(outside.join("secret.js"), root.join("escape.js")).unwrap();
        symlink(root.join("physical.css"), root.join("logical.js")).unwrap();

        let entry = resolve_asset_request(&canonical_root, TRUSTED_ENTRY_URI).unwrap();
        assert_eq!(
            entry.canonical,
            root.join("index.html").canonicalize().unwrap()
        );
        assert_eq!(entry.logical, Path::new("index.html"));
        assert_eq!(entry.length, 4);

        let contained =
            resolve_asset_request(&canonical_root, "html-desktop-shell://assets/logical.js")
                .unwrap();
        assert_eq!(
            contained.canonical,
            root.join("physical.css").canonicalize().unwrap()
        );
        assert_eq!(contained.logical, Path::new("logical.js"));
        assert_eq!(
            mime_type_for_logical_path(&contained.logical),
            "text/javascript"
        );
        assert!(
            resolve_asset_request(&canonical_root, "html-desktop-shell://assets/escape.js")
                .is_err()
        );

        fs::remove_dir_all(base).unwrap();
    }
}
