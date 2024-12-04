use crate::{
    player::{self, actions::Action, notification::Notification},
    service::{Album, Artist, Favorites, Playlist, SearchResults},
};
use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query,
    },
    http::{header, Method, Request, Response},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures::{SinkExt, StreamExt};
use include_dir::{include_dir, Dir};
use mime_guess::{mime::HTML, MimeGuess};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{net::SocketAddr, path::PathBuf, str::FromStr};
use tokio::select;
use tower_http::cors::{Any, CorsLayer};

static SITE: Dir = include_dir!("$CARGO_MANIFEST_DIR/../www/build");

pub async fn init(binding_interface: SocketAddr) {
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_origin(Any);

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/*key", get(static_handler))
        .route("/api/artists/:id", get(artist))
        .route("/api/albums/:id", get(album))
        .route("/api/artists/:id/releases", get(artist_releases))
        .route("/api/playlist/:id", get(playlist))
        .route("/api/search", get(search))
        .route("/api/favorites", get(favorites))
        .route("/api/favorite-playlists", get(favorite_playlists))
        .route(
            "/api/favorite/album/:id",
            post(add_favorite_album).delete(remove_favorite_album),
        )
        .route(
            "/api/favorite/artist/:id",
            post(add_favorite_artist).delete(remove_favorite_artist),
        )
        .route(
            "/api/favorite/playlist/:id",
            post(add_favorite_playlist).delete(remove_favorite_playlist),
        )
        .route("/", get(static_handler))
        .layer(cors);

    debug!("listening on {}", binding_interface);

    let listener = tokio::net::TcpListener::bind(&binding_interface)
        .await
        .unwrap();

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let mut broadcast_receiver = player::notify_receiver();

            loop {
                if let Some(message) = broadcast_receiver.next().await {
                    if message == Notification::Quit {
                        break;
                    }
                }
            }
        })
        .await
        .unwrap();
}

#[derive(Deserialize)]
struct SearchQuery {
    query: String,
}

async fn add_favorite_album(Path(id): Path<String>) {
    player::add_favorite_album(id).await;
}

async fn remove_favorite_album(Path(id): Path<String>) {
    player::remove_favorite_album(id).await;
}

async fn add_favorite_artist(Path(id): Path<String>) {
    player::add_favorite_artist(id).await;
}

async fn remove_favorite_artist(Path(id): Path<String>) {
    player::remove_favorite_artist(id).await;
}

async fn add_favorite_playlist(Path(id): Path<String>) {
    player::add_favorite_playlist(id).await;
}

async fn remove_favorite_playlist(Path(id): Path<String>) {
    println!("remove playlist {id}");
    player::remove_favorite_playlist(id).await;
}

async fn favorites() -> Json<Favorites> {
    let results = player::favorites().await;
    Json(results)
}

async fn favorite_playlists() -> Json<Vec<Playlist>> {
    let results = player::user_playlists().await;
    Json(results)
}

async fn search(query: Query<SearchQuery>) -> Json<SearchResults> {
    let results = player::search(&query.query).await;
    Json(results)
}

async fn artist(Path(id): Path<i32>) -> Json<Artist> {
    let results = player::artist(id).await;
    Json(results)
}

async fn album(Path(id): Path<String>) -> Json<Album> {
    let results = player::album(id).await;
    Json(results)
}

async fn artist_releases(Path(id): Path<i32>) -> Json<Vec<Album>> {
    let results = player::artist_albums(id).await;
    Json(results)
}

async fn playlist(Path(id): Path<i64>) -> Json<Playlist> {
    let results = player::playlist(id).await;
    Json(results)
}

async fn static_handler(req: Request<Body>) -> impl IntoResponse {
    let req_path = req.uri().path();
    let mut path = PathBuf::from_str(&req_path[1..]).expect("error parsing path");

    // If it's a directory, search for an index.html file.
    if path.is_dir() || req.uri().path() == "/" {
        path.push("index.html");
    }

    // Get the extension or empty string if none.
    let extension = path
        .extension()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default();

    // Attempt to guess the mime type of the file based on the file extension.
    // If one can't be guessed, used text/plain.
    let mime_type = if let Some(mime) = MimeGuess::from_ext(extension).first() {
        mime.essence_str().to_string()
    } else {
        "text/plain".to_string()
    };

    // Attempt to retrieve the necessary file from the embedded path.
    let (body, mime_type, status) = if let Some(file) = SITE.get_file(&path) {
        (Body::from(file.contents().to_vec()), mime_type, 200)
    } else {
        (
            Body::from("<html><body><h1>404</h1></body></html>"),
            HTML.as_str().to_string(),
            404,
        )
    };

    Response::builder()
        .header(header::CONTENT_TYPE, mime_type)
        .status(status)
        .body(body)
        .expect("error making body")
}

async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_connection)
}

async fn handle_connection(socket: WebSocket) {
    debug!("new websocket connection");
    let (mut sender, mut receiver) = socket.split();
    let (rt_sender, rt_receiver) = flume::bounded::<Value>(1);

    let mut send_task = tokio::spawn(async move {
        debug!("spawning send task");
        let mut broadcast_receiver = player::notify_receiver();

        if let Ok(ct) = serde_json::to_string(&Notification::CurrentTrackList {
            list: player::current_tracklist().await,
        }) {
            sender.send(Message::Text(ct)).await.expect("error");
        }

        if let Some(position) = player::position() {
            if let Ok(p) = serde_json::to_string(&Notification::Position { clock: position }) {
                sender.send(Message::Text(p)).await.expect("error");
            }
        }

        if let Ok(s) = serde_json::to_string(&Notification::Status {
            status: player::current_state(),
        }) {
            sender.send(Message::Text(s)).await.expect("error");
        }

        let mut rt_stream = rt_receiver.stream();

        loop {
            select! {
                Some(message) = broadcast_receiver.next() => {
                    let json = serde_json::to_string(&message).expect("error making json");
                    match sender.send(Message::Text(json)).await {
                        Ok(()) => {}
                        Err(error) => {
                            debug!(?error)
                        }
                    }
                }
                Some(response) = rt_stream.next() => {
                    let json = serde_json::to_string(&response).expect("error making json");
                    match sender.send(Message::Text(json)).await {
                        Ok(()) => {}
                        Err(error) => {
                            debug!(?error)
                        }
                    }
                }
            }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        debug!("spawning receive task");

        while let Some(data) = receiver.next().await {
            match data {
                Ok(message) => {
                    if let Message::Text(s) = message {
                        if let Ok(action) = serde_json::from_str::<Action>(&s) {
                            debug!(?action);
                            match action {
                                Action::Play => player::play().await.expect(""),
                                Action::Pause => player::pause().await.expect(""),
                                Action::PlayPause => player::play_pause().await.expect(""),
                                Action::Next => player::next().await.expect(""),
                                Action::Previous => player::previous().await.expect(""),
                                Action::Stop => player::stop().await.expect(""),
                                Action::Quit => player::quit().await.expect(""),
                                Action::SkipTo { num } => player::skip(num, true).await.expect(""),
                                Action::JumpForward => player::jump_forward().await.expect(""),
                                Action::JumpBackward => player::jump_backward().await.expect(""),
                                Action::PlayAlbum { album_id } => {
                                    player::play_album(&album_id).await.expect("")
                                }
                                Action::PlayTrack { track_id } => {
                                    player::play_track(track_id).await.expect("")
                                }
                                Action::PlayUri { uri } => player::play_uri(&uri).await.expect(""),
                                Action::PlayPlaylist { playlist_id } => {
                                    player::play_playlist(playlist_id).await.expect("")
                                }
                                Action::Search { query } => {
                                    let results = player::search(&query).await;
                                    match rt_sender
                                        .send_async(
                                            json!({ "searchResults": { "results": results }}),
                                        )
                                        .await
                                    {
                                        Ok(_) => {}
                                        Err(error) => {
                                            debug!("error sending response {}", error)
                                        }
                                    }
                                }
                                Action::FetchArtistAlbums { artist_id } => {
                                    let results = player::artist_albums(artist_id).await;
                                    match rt_sender
                                        .send_async(
                                            json!({ "artistAlbums": { "id": artist_id, "albums": results }}),
                                        )
                                        .await
                                    {
                                        Ok(_) => {}
                                        Err(error) => debug!("error sending response {}", error),
                                    }
                                }
                                Action::FetchPlaylistTracks { playlist_id } => {
                                    let results = player::playlist_tracks(playlist_id).await;
                                    match rt_sender
                                        .send_async(
                                            json!({ "playlistTracks": { "id": playlist_id, "tracks": results } })
                                        )
                                        .await
                                    {
                                        Ok(_) => {}
                                        Err(error) => debug!("error sending response {}", error),
                                    }
                                }
                                Action::FetchUserPlaylists => {
                                    let results = player::user_playlists().await;
                                    match rt_sender
                                        .send_async(json!({ "userPlaylists": results }))
                                        .await
                                    {
                                        Ok(_) => {}
                                        Err(error) => debug!("error sending response {}", error),
                                    }
                                }
                            }
                        };
                    }
                }
                Err(err) => {
                    debug!(?err)
                }
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
}
