mod cli;
mod mpris;
mod player;
mod qobuz;
mod state;
mod ui;

extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use crate::{
    cli::{Cli, Commands},
    player::Playlist,
    qobuz::{client, PlaylistTrack},
    state::app::PlayerKey,
};
use clap::Parser;
use dialoguer::{console::Term, theme::ColorfulTheme, Confirm, Input, Password, Select};
use std::time::Duration;

use self::{
    cli::ConfigCommands,
    player::AudioQuality,
    state::{
        app::{AppKey, ClientKey},
        StringValue,
    },
};

#[tokio::main]
async fn main() -> Result<(), String> {
    pretty_env_logger::init();
    let cli = Cli::parse();
    let mut base_dir = dirs::data_local_dir().unwrap();
    base_dir.push("hifi-rs");

    // SETUP DATABASE
    let app_state = state::app::new(base_dir);

    // CLI COMMANDS
    match cli.command {
        Commands::Resume { no_tui } => {
            if let (Some(playlist), Some(next_up)) = (
                app_state
                    .player
                    .get::<String, Playlist>(AppKey::Player(PlayerKey::Playlist)),
                app_state
                    .player
                    .get::<String, PlaylistTrack>(AppKey::Player(PlayerKey::NextUp)),
            ) {
                if let Some(track_url) = next_up.track_url {
                    let (mut player, broadcast) = player::new(app_state.clone());

                    let mut client = client::new(app_state.clone()).await;
                    client.setup(cli.username, cli.password).await;

                    player.setup(client, true).await;

                    if let Some(prev_playlist) = app_state
                        .player
                        .get::<String, Playlist>(AppKey::Player(PlayerKey::PreviousPlaylist))
                    {
                        player.set_prev_playlist(prev_playlist);
                    }

                    player.set_playlist(playlist);
                    player.set_uri(track_url);

                    player.play();

                    if no_tui {
                        let mut quitter = app_state.quitter();

                        ctrlc::set_handler(move || {
                            app_state.send_quit();
                            std::process::exit(0);
                        })
                        .expect("error setting ctrlc handler");

                        loop {
                            if let Ok(quit) = quitter.try_recv() {
                                if quit {
                                    debug!("quitting");
                                    break;
                                }
                            }
                            std::thread::sleep(Duration::from_millis(hifi_rs::REFRESH_RESOLUTION));
                        }
                    } else {
                        let mut tui = ui::terminal::new();
                        tui.event_loop(broadcast, player).await;
                    }
                } else {
                    error!("Track is missing url.");
                }
            } else {
                println!("Sorry, the previous session could not be resumed.");
            }

            Ok(())
        }
        Commands::Play { query, quality } => {
            let (player, broadcast) = player::new(app_state.clone());

            let mut client = client::new(app_state.clone()).await;
            client.setup(cli.username, cli.password).await;

            client.check_auth().await;

            if let Some(mut results) = client.search_albums(query, 100).await {
                let album_list = results
                    .albums
                    .items
                    .iter()
                    .map(|i| {
                        format!(
                            "{} - {} ({})",
                            i.title,
                            i.artist.name,
                            i.release_date_original.get(0..4).unwrap()
                        )
                    })
                    .collect::<Vec<String>>();

                let selected = Select::with_theme(&ColorfulTheme::default())
                    .items(&album_list)
                    .default(0)
                    .max_length(10)
                    .interact_on_opt(&Term::stderr())
                    .expect("problem getting selection");

                if let Some(index) = selected {
                    let selected_album = results.albums.items.remove(index);

                    app_state.player.clear();
                    player.setup(client.clone(), false).await;

                    let quality = if let Some(q) = quality {
                        q
                    } else {
                        client.quality()
                    };

                    if let Some(album) = client.album(selected_album.id).await {
                        player.play_album(album, quality, client.clone()).await;

                        let mut tui = ui::terminal::new();
                        tui.event_loop(broadcast, player).await;
                    }
                }

                Ok(())
            } else {
                Err("".to_string())
            }
        }
        Commands::Search { query } => {
            let mut client = client::new(app_state.clone()).await;
            client.setup(cli.username, cli.password).await;

            client.check_auth().await;
            if let Some(results) = client.search_all(query).await {
                //let json = serde_json::to_string(&results);
                print!("{}", results);
                Ok(())
            } else {
                Err("".to_string())
            }
        }
        Commands::SearchAlbums { query } => {
            let mut client = client::new(app_state.clone()).await;
            client.setup(cli.username, cli.password).await;

            client.check_auth().await;
            if let Some(results) = client.search_albums(query, 10).await {
                let json = serde_json::to_string(&results);
                print!("{}", json.expect("failed to convert results to string"));
                Ok(())
            } else {
                Err("".to_string())
            }
        }
        Commands::GetAlbum { id } => {
            let mut client = client::new(app_state.clone()).await;
            client.setup(cli.username, cli.password).await;

            client.check_auth().await;
            if let Some(results) = client.album(id).await {
                let json = serde_json::to_string(&results);
                print!("{}", json.expect("failed to convert results to string"));
                Ok(())
            } else {
                Err("".to_string())
            }
        }
        Commands::SearchArtists { query } => {
            let mut client = client::new(app_state.clone()).await;
            client.setup(cli.username, cli.password).await;

            client.check_auth().await;
            if let Some(results) = client.search_artists(query).await {
                let json = serde_json::to_string(&results);
                print!("{}", json.expect("failed to convert results to string"));
                Ok(())
            } else {
                Err("".to_string())
            }
        }
        Commands::GetArtist { id } => {
            let mut client = client::new(app_state.clone()).await;
            client.setup(cli.username, cli.password).await;

            client.check_auth().await;
            if let Some(results) = client.artist(id).await {
                let json = serde_json::to_string(&results);
                print!("{}", json.expect("failed to convert results to string"));
                Ok(())
            } else {
                Err("".to_string())
            }
        }
        Commands::GetTrack { id } => {
            let mut client = client::new(app_state.clone()).await;
            client.setup(cli.username, cli.password).await;

            client.check_auth().await;
            if let Some(results) = client.track(id).await {
                let json = serde_json::to_string(&results);
                print!("{}", json.expect("failed to convert results to string"));
                Ok(())
            } else {
                Err("".to_string())
            }
        }
        Commands::TrackURL { id, quality } => {
            let mut client = client::new(app_state.clone()).await;
            client.setup(cli.username, cli.password).await;

            client.check_auth().await;
            match client.track_url(id, quality.clone(), None).await {
                Ok(result) => {
                    let json = serde_json::to_string(&result);
                    print!("{}", json.expect("failed to convert results to string"));
                    Ok(())
                }
                Err(error) => Err(error),
            }
        }
        Commands::MyPlaylists {} => {
            let mut client = client::new(app_state.clone()).await;
            client.setup(cli.username, cli.password).await;

            client.check_auth().await;
            if let Some(results) = client.user_playlists().await {
                let json = serde_json::to_string(&results);
                print!("{}", json.expect("failed to convert results to string"));
                Ok(())
            } else {
                Err("".to_string())
            }
        }
        Commands::Playlist { playlist_id } => {
            let mut client = client::new(app_state.clone()).await;
            client.setup(cli.username, cli.password).await;

            client.check_auth().await;
            if let Some(results) = client.playlist(playlist_id).await {
                let json = serde_json::to_string(&results);
                print!("{}", json.expect("failed to convert results to string"));
                Ok(())
            } else {
                Err("".to_string())
            }
        }
        Commands::StreamTrack { track_id, quality } => {
            let (player, broadcast) = player::new(app_state.clone());

            let mut client = client::new(app_state.clone()).await;
            client.setup(cli.username, cli.password).await;

            client.check_auth().await;
            if let Some(track) = client.track(track_id.to_string()).await {
                app_state.player.clear();
                player.setup(client.clone(), false).await;
                player.play_track(track, quality.unwrap(), client).await;

                let mut tui = ui::terminal::new();
                tui.event_loop(broadcast, player).await;
            }

            Ok(())
        }
        Commands::StreamAlbum {
            album_id,
            quality,
            no_tui,
        } => {
            let (player, broadcast) = player::new(app_state.clone());

            let mut client = client::new(app_state.clone()).await;
            client.setup(cli.username, cli.password).await;

            client.check_auth().await;
            if let Some(album) = client.album(album_id).await {
                app_state.player.clear();
                player.setup(client.clone(), false).await;

                let quality = if let Some(q) = quality {
                    q
                } else {
                    client.quality()
                };

                player.play_album(album, quality, client.clone()).await;

                if no_tui {
                    let mut quitter = app_state.quitter();

                    ctrlc::set_handler(move || {
                        app_state.send_quit();
                        std::process::exit(0);
                    })
                    .expect("error setting ctrlc handler");

                    loop {
                        if let Ok(quit) = quitter.try_recv() {
                            if quit {
                                debug!("quitting");
                                break;
                            }
                        }
                        std::thread::sleep(Duration::from_millis(hifi_rs::REFRESH_RESOLUTION));
                    }
                } else {
                    let mut tui = ui::terminal::new();
                    tui.event_loop(broadcast, player).await;
                }
            }

            Ok(())
        }
        Commands::Download { id, quality } => {
            // SETUP API CLIENT
            let mut client = client::new(app_state.clone()).await;
            client.setup(cli.username, cli.password).await;

            client.check_auth().await;
            if let Ok(result) = client.track_url(id, quality.clone(), None).await {
                client.download(result).await;
                Ok(())
            } else {
                Err("".to_string())
            }
        }
        Commands::Config { command } => match command {
            ConfigCommands::Username {} => {
                let username: String = Input::new()
                    .with_prompt("Enter your username / email")
                    .interact_text()
                    .expect("failed to get username");

                app_state.config.insert::<String, StringValue>(
                    AppKey::Client(ClientKey::Username),
                    username.into(),
                );

                println!("Username saved.");

                Ok(())
            }
            ConfigCommands::Password {} => {
                let password: String = Password::new()
                    .with_prompt("Enter your password (hidden)")
                    .interact()
                    .expect("failed to get password");

                let md5_pw = format!("{:x}", md5::compute(password));

                debug!("saving password to database: {}", md5_pw);

                app_state.config.insert::<String, StringValue>(
                    AppKey::Client(ClientKey::Password),
                    md5_pw.into(),
                );

                println!("Password saved.");

                Ok(())
            }
            ConfigCommands::DefaultQuality { quality } => {
                app_state.config.insert::<String, AudioQuality>(
                    AppKey::Client(ClientKey::DefaultQuality),
                    quality,
                );

                println!("Default quality saved.");

                Ok(())
            }
            ConfigCommands::Clear {} => {
                if Confirm::new()
                    .with_prompt("This will clear the configuration in the database.\nDo you want to continue?")
                    .interact()
                    .expect("failed to get response")
                {
                    app_state.config.clear();

                    println!("Database cleared.");

                    Ok(())
                } else {
                    Ok(())
                }
            }
        },
    }
}