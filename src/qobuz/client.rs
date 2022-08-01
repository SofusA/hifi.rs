use super::{
    Album, AlbumSearchResults, Artist, ArtistSearchResults, Playlist, Track, TrackURL,
    UserPlaylists,
};
use crate::{
    player::AudioQuality,
    state::{
        app::{AppKey, AppState, ClientKey},
        StringValue,
    },
};
use hifi_rs::capitalize;
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Method, Response, StatusCode,
};
use serde_json::Value;
use std::{collections::HashMap, fs::File};
use tokio_stream::StreamExt;

const BUNDLE_REGEX: &str =
    r#"<script src="(/resources/\d+\.\d+\.\d+-[a-z]\d{3}/bundle\.js)"></script>"#;
const APP_REGEX: &str = r#"cluster:"eu"}\):\(n.qobuzapi=\{app_id:"(?P<app_id>\d{9})",app_secret:"\w{32}",base_port:"80",base_url:"https://www\.qobuz\.com",base_method:"/api\.json/0\.2/"},n"#;
const SEED_REGEX: &str =
    r#"[a-z]\.initialSeed\("(?P<seed>[\w=]+)",window\.utimezone\.(?P<timezone>[a-z]+)\)"#;

macro_rules! format_info {
    () => {
        r#"name:"\w+/(?P<timezone>{}([a-z]?))",info:"(?P<info>[\w=]+)",extras:"(?P<extras>[\w=]+)""#
    };
}

#[derive(Debug, Clone)]
pub struct Client {
    secrets: HashMap<String, String>,
    active_secret: Option<StringValue>,
    app_id: Option<StringValue>,
    username: Option<StringValue>,
    password: Option<StringValue>,
    base_url: String,
    client: reqwest::Client,
    default_quality: AudioQuality,
    user_token: Option<StringValue>,
    bundle_regex: regex::Regex,
    app_id_regex: regex::Regex,
    seed_regex: regex::Regex,
    state: AppState,
}

pub async fn new(state: AppState) -> Client {
    let mut headers = HeaderMap::new();
    headers.insert(
            "User-Agent",
            HeaderValue::from_str(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/102.0.0.0 Safari/537.36",
            )
            .unwrap(),
        );

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(headers)
        .build()
        .unwrap();

    let default_quality = if let Some(quality) = state
        .config
        .get::<String, AudioQuality>(AppKey::Client(ClientKey::DefaultQuality))
    {
        quality
    } else {
        AudioQuality::Mp3
    };

    Client {
        client,
        secrets: HashMap::new(),
        active_secret: None,
        user_token: None,
        app_id: None,
        username: None,
        state,
        password: None,
        default_quality,
        base_url: "https://www.qobuz.com/api.json/0.2/".to_string(),
        bundle_regex: regex::Regex::new(BUNDLE_REGEX).unwrap(),
        app_id_regex: regex::Regex::new(APP_REGEX).unwrap(),
        seed_regex: regex::Regex::new(SEED_REGEX).unwrap(),
    }
}

#[non_exhaustive]
enum Endpoint {
    Album,
    Artist,
    Login,
    Track,
    UserPlaylist,
    SearchArtists,
    SearchAlbums,
    TrackURL,
    Playlist,
    Search,
}

impl Endpoint {
    fn as_str(&self) -> &'static str {
        match self {
            Endpoint::Album => "album/get",
            Endpoint::Artist => "artist/get",
            Endpoint::Login => "user/login",
            Endpoint::Track => "track/get",
            Endpoint::SearchArtists => "artist/search",
            Endpoint::UserPlaylist => "playlist/getUserPlaylists",
            Endpoint::SearchAlbums => "album/search",
            Endpoint::Search => "catalog/search",
            Endpoint::TrackURL => "track/getFileUrl",
            Endpoint::Playlist => "playlist/get",
        }
    }
}

#[allow(dead_code)]
impl Client {
    pub fn quality(&self) -> AudioQuality {
        self.default_quality.clone()
    }
    /// Setup app_id, secret and user credentials for authentication
    pub async fn setup(&mut self, username: Option<String>, password: Option<String>) {
        info!("setting up the api client");

        let mut refresh_config = false;

        if let Some(app_id) = self
            .state
            .config
            .get::<String, StringValue>(AppKey::Client(ClientKey::AppID))
        {
            info!("using app_id from cache: {}", app_id);
            self.set_app_id(Some(app_id));

            if let Some(active_secret) = self
                .state
                .config
                .get::<String, StringValue>(AppKey::Client(ClientKey::ActiveSecret))
            {
                info!("using app_secret from cache: {}", active_secret);
                self.set_active_secret(Some(active_secret));
            } else {
                self.set_active_secret(None);
                self.set_app_id(None);
                refresh_config = true;
            }
        } else {
            self.set_app_id(None);
            refresh_config = true;
        }

        if refresh_config {
            self.get_config().await;
        }

        if let Some(token) = self
            .state
            .config
            .get::<String, StringValue>(AppKey::Client(ClientKey::Token))
        {
            info!("using token from cache");
            self.set_token(token);
            return;
        }

        if let Some(u) = username {
            debug!("using username from cli argument: {}", u);
            self.set_username(u.into());
        } else if let Some(u) = self
            .state
            .config
            .get::<String, StringValue>(AppKey::Client(ClientKey::Username))
        {
            debug!("using username stored in database: {}", u);
            self.set_username(u);
        } else {
            println!("No username.");
            std::process::exit(1);
        }

        if let Some(p) = password {
            debug!("using password from cli argument: {}", p);
            self.set_password(p.into());
        } else if let Some(p) = self
            .state
            .config
            .get::<String, StringValue>(AppKey::Client(ClientKey::Password))
        {
            debug!("using password stored in database: {}", p);
            self.set_password(p);
        } else {
            println!("No password.");
            std::process::exit(1);
        }
    }

    /// Login a user
    pub async fn login(&mut self) -> Option<String> {
        let endpoint = format!("{}{}", self.base_url, Endpoint::Login.as_str());
        let app_id = self.app_id.clone().unwrap();
        let username = self
            .username
            .clone()
            .expect("tried to login without username.");
        let password = self
            .password
            .clone()
            .expect("tried to login without password.");

        info!(
            "logging in with email ({}) and password **HIDDEN** for app_id {}",
            username, app_id
        );

        let params = vec![
            ("email", username.as_str()),
            ("password", password.as_str()),
            ("app_id", app_id.as_str()),
        ];

        match self.make_call(endpoint, Some(params)).await {
            Ok(response) => {
                let json: Value = serde_json::from_str(response.as_str()).unwrap();
                info!("Successfully logged in");
                debug!("{}", json);
                let mut token = json["user_auth_token"].to_string();
                token = token[1..token.len() - 1].to_string();

                self.user_token = Some(token.clone().into());
                self.state.config.insert::<String, StringValue>(
                    AppKey::Client(ClientKey::Token),
                    token.clone().into(),
                );
                Some(token)
            }
            Err(_) => {
                println!("ERROR: Invalid username/email and password combination.");
                std::process::exit(1);
            }
        }
    }

    /// Retrieve a list of the user's playlists
    pub async fn user_playlists(&mut self) -> Option<UserPlaylists> {
        let endpoint = format!("{}{}", self.base_url, Endpoint::UserPlaylist.as_str());
        let params = vec![("limit", "500"), ("extra", "tracks"), ("offset", "0")];

        if let Ok(response) = self.make_call(endpoint, Some(params)).await {
            let playlist_response: UserPlaylists = serde_json::from_str(response.as_str()).unwrap();

            Some(playlist_response)
        } else {
            None
        }
    }

    /// Retrieve a playlist
    pub async fn playlist(&mut self, playlist_id: String) -> Option<Playlist> {
        let endpoint = format!("{}{}", self.base_url, Endpoint::Playlist.as_str());
        let params = vec![
            ("limit", "500"),
            ("extra", "tracks"),
            ("playlist_id", playlist_id.as_str()),
            ("offset", "0"),
        ];

        if let Ok(response) = self.make_call(endpoint, Some(params)).await {
            let playlist = serde_json::from_str(response.as_str()).unwrap();

            Some(playlist)
        } else {
            None
        }
    }

    /// Retrieve track information
    pub async fn track(&mut self, track_id: String) -> Option<Track> {
        let endpoint = format!("{}{}", self.base_url, Endpoint::Track.as_str());
        let params = vec![("track_id", track_id.as_str())];

        if let Ok(response) = self.make_call(endpoint, Some(params)).await {
            let track_info: Track = serde_json::from_str(response.as_str()).unwrap();
            Some(track_info)
        } else {
            None
        }
    }

    /// Retrieve url information for a track's audio file
    pub async fn track_url(
        &mut self,
        track_id: i32,
        fmt_id: Option<AudioQuality>,
        sec: Option<String>,
    ) -> Result<TrackURL, String> {
        let endpoint = format!("{}{}", self.base_url, Endpoint::TrackURL.as_str());
        let now = format!("{}", chrono::Utc::now().timestamp());
        let secret = if let Some(secret) = sec {
            StringValue::from(secret)
        } else if let Some(secret) = &self.active_secret {
            secret.clone()
        } else {
            println!("The secret needed to fetch the track url could not be found.");
            std::process::exit(1);
        };

        let format_id = if let Some(quality) = fmt_id {
            quality
        } else {
            self.quality()
        };

        let sig = format!(
            "trackgetFileUrlformat_id{}intentstreamtrack_id{}{}{}",
            format_id.clone(),
            track_id,
            now,
            secret
        );
        let hashed_sig = format!("{:x}", md5::compute(sig.as_str()));

        let track_id = track_id.to_string();
        let format_string = format_id.to_string();

        let params = vec![
            ("request_ts", now.as_str()),
            ("request_sig", hashed_sig.as_str()),
            ("track_id", track_id.as_str()),
            ("format_id", format_string.as_str()),
            ("intent", "stream"),
        ];

        match self.make_call(endpoint, Some(params)).await {
            Ok(response) => {
                let track_url: TrackURL = serde_json::from_str(response.as_str()).unwrap();
                Ok(track_url)
            }
            Err(response) => Err(response),
        }
    }

    pub async fn search_all(&mut self, query: String) -> Option<String> {
        let endpoint = format!("{}{}", self.base_url, Endpoint::Search.as_str());
        let params = vec![("query", query.as_str()), ("limit", "500")];

        if let Ok(response) = self.make_call(endpoint, Some(params)).await {
            //let album: Album = serde_json::from_str(response.as_str()).unwrap();
            Some(response)
        } else {
            None
        }
    }

    // Retrieve information about an album
    pub async fn album(&mut self, album_id: String) -> Option<Album> {
        let endpoint = format!("{}{}", self.base_url, Endpoint::Album.as_str());
        let params = vec![("album_id", album_id.as_str())];

        if let Ok(response) = self.make_call(endpoint, Some(params)).await {
            let album: Album = serde_json::from_str(response.as_str()).unwrap();
            Some(album)
        } else {
            None
        }
    }

    // Search the database for albums
    pub async fn search_albums(&mut self, query: String, limit: i32) -> Option<AlbumSearchResults> {
        let endpoint = format!("{}{}", self.base_url, Endpoint::SearchAlbums.as_str());
        let limit = limit.to_string();
        let params = vec![("query", query.as_str()), ("limit", limit.as_str())];

        if let Ok(response) = self.make_call(endpoint, Some(params)).await {
            let results: AlbumSearchResults = serde_json::from_str(response.as_str()).unwrap();
            Some(results)
        } else {
            None
        }
    }

    // Retrieve information about an artist
    pub async fn artist(&mut self, artist_id: String) -> Option<Artist> {
        let endpoint = format!("{}{}", self.base_url, Endpoint::Artist.as_str());
        let app_id = self.app_id.clone();
        let params = vec![
            ("artist_id", artist_id.as_str()),
            (
                "app_id",
                app_id
                    .as_ref()
                    .expect("missing app id. this should not have happened.")
                    .as_str(),
            ),
            ("limit", "500"),
            ("offset", "0"),
            ("extra", "albums"),
        ];

        if let Ok(response) = self.make_call(endpoint, Some(params)).await {
            let artist: Artist = serde_json::from_str(response.as_str()).unwrap();
            Some(artist)
        } else {
            None
        }
    }

    // Search the database for artists
    pub async fn search_artists(&mut self, query: String) -> Option<ArtistSearchResults> {
        let endpoint = format!("{}{}", self.base_url, Endpoint::SearchArtists.as_str());
        let params = vec![("query", query.as_str()), ("limit", "500")];

        if let Ok(response) = self.make_call(endpoint, Some(params)).await {
            let results: ArtistSearchResults = serde_json::from_str(response.as_str()).unwrap();
            Some(results)
        } else {
            None
        }
    }

    // Download a track to disk
    pub async fn download(&self, track: TrackURL) {
        let response = self.client.get(track.url).send().await.unwrap();
        let mut output_file = File::create(format!("{}.flac", track.track_id)).unwrap();
        let total_size = response
            .headers()
            .get("Content-Length")
            .expect("failed to get content-length header")
            .to_str()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        let mut size_left = total_size;

        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            if let Ok(c) = chunk {
                size_left -= c.len() as f64;
                let percentage_left = 1. - size_left / total_size;
                debug!("progress: {}%", (percentage_left * 100.) as i32);
                std::io::copy(&mut c.to_vec().as_slice(), &mut output_file).unwrap();
            }
        }
    }

    // Set a user access token for authentication
    fn set_token(&mut self, token: StringValue) {
        self.user_token = Some(token);
    }

    // Set a username for authentication
    fn set_username(&mut self, username: StringValue) {
        self.username = Some(username);
    }

    // Set a password for authentication
    fn set_password(&mut self, password: StringValue) {
        self.password = Some(password);
    }

    // Set an app_id for authentication
    fn set_app_id(&mut self, app_id: Option<StringValue>) {
        self.app_id = app_id;
    }

    // Set an app secret for authentication
    fn set_active_secret(&mut self, active_secret: Option<StringValue>) {
        self.active_secret = active_secret;
    }

    // Verify that the client has the needed
    // credentials to access the api.
    pub async fn check_auth(&mut self) {
        if self.app_id.is_none() {
            self.get_config().await;
        }

        if self.active_secret.is_none() {
            self.test_secrets().await;
        }

        if self.username.is_some() && self.password.is_some() {
            self.login().await;
        } else if self.user_token.is_none() {
            println!("Username and password required.");
            std::process::exit(1);
        }
    }

    // Call the api and retrieve the JSON payload
    async fn make_call(
        &mut self,
        endpoint: String,
        params: Option<Vec<(&str, &str)>>,
    ) -> Result<String, String> {
        let mut headers = HeaderMap::new();

        if let Some(app_id) = &self.app_id {
            info!("adding app_id to request headers: {}", app_id);
            headers.insert("X-App-Id", HeaderValue::from_str(app_id.as_str()).unwrap());
        }

        if let Some(token) = &self.user_token {
            info!("adding token to request headers: {}", token);
            headers.insert(
                "X-User-Auth-Token",
                HeaderValue::from_str(token.as_str()).unwrap(),
            );
        }

        let request = self.client.request(Method::GET, endpoint).headers(headers);

        if let Some(p) = params {
            let response = request.query(&p).send().await;
            match response {
                Ok(r) => self.handle_response(r).await,
                Err(err) => {
                    error!("call to api failed: {}", err.to_string());
                    Err(err.to_string())
                }
            }
        } else {
            let response = request.send().await;
            match response {
                Ok(r) => self.handle_response(r).await,
                Err(err) => {
                    error!("call to api failed: {}", err.to_string());
                    Err(err.to_string())
                }
            }
        }
    }

    // Handle a response retrieved from the api
    async fn handle_response(&mut self, response: Response) -> Result<String, String> {
        match response.status() {
            StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED | StatusCode::NOT_FOUND => {
                let res = response.text().await.unwrap();
                debug!("{}", res);
                Err(res)
            }
            StatusCode::OK => {
                let res = response.text().await.unwrap();
                Ok(res)
            }
            _ => unreachable!(),
        }
    }

    // ported from https://github.com/vitiko98/qobuz-dl/blob/master/qobuz_dl/bundle.py
    // Retrieve the app_id and generate the secrets needed to authenticate
    async fn get_config(&mut self) {
        let play_url = "https://play.qobuz.com";
        let login_page = self
            .client
            .get(format!("{}/login", play_url))
            .send()
            .await
            .expect("failed to get login page. something is very wrong.");

        let contents = login_page.text().await.unwrap();

        let bundle_path = self
            .bundle_regex
            .captures(contents.as_str())
            .expect("regex failed")
            .get(1)
            .map_or("", |m| m.as_str());

        let bundle_url = format!("{}{}", play_url, bundle_path);
        let bundle_page = self.client.get(bundle_url).send().await.unwrap();

        let bundle_contents = bundle_page.text().await.unwrap();

        let app_id: StringValue = self
            .app_id_regex
            .captures(bundle_contents.as_str())
            .expect("regex failed")
            .name("app_id")
            .map_or("".to_string(), |m| m.as_str().to_string())
            .into();

        self.app_id = Some(app_id.clone());
        self.state
            .config
            .insert::<String, StringValue>(AppKey::Client(ClientKey::AppID), app_id.clone());

        let seed_data = self.seed_regex.captures_iter(bundle_contents.as_str());

        seed_data.for_each(|s| {
            let seed = s.name("seed").map_or("", |m| m.as_str()).to_string();
            let timezone = s.name("timezone").map_or("", |m| m.as_str()).to_string();

            let info_regex = format!(format_info!(), capitalize(&timezone));
            let info_regex_str = info_regex.as_str();
            regex::Regex::new(info_regex_str)
                .unwrap()
                .captures_iter(bundle_contents.as_str())
                .for_each(|c| {
                    let timezone = c.name("timezone").map_or("", |m| m.as_str()).to_string();
                    let info = c.name("info").map_or("", |m| m.as_str()).to_string();
                    let extras = c.name("extras").map_or("", |m| m.as_str()).to_string();

                    let chars = format!("{}{}{}", seed, info, extras);
                    let encoded_secret = chars[..chars.len() - 44].to_string();
                    let decoded_secret =
                        base64::decode(encoded_secret).expect("failed to decode base64 secret");
                    let secret_utf8 = std::str::from_utf8(&decoded_secret)
                        .expect("failed to convert base64 to string")
                        .to_string();

                    debug!("{}\t{}\t{}", app_id, timezone.to_lowercase(), secret_utf8);
                    self.secrets.insert(timezone, secret_utf8);
                });
        });
    }

    // Check the retrieved secrets to see which one works.
    async fn test_secrets(&mut self) {
        debug!("testing secrets");
        let secrets = self.secrets.clone();

        for (timezone, secret) in secrets.iter() {
            let response = self
                .track_url(5966783, Some(AudioQuality::Mp3), Some(secret.to_string()))
                .await;

            if response.is_ok() {
                debug!("found good secret: {}\t{}", timezone, secret);
                let secret_string = secret.to_string();
                self.state.config.insert::<String, StringValue>(
                    AppKey::Client(ClientKey::ActiveSecret),
                    secret_string.clone().into(),
                );
                self.active_secret = Some(secret_string.into());
            }
        }
    }
}