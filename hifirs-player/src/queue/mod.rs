pub mod controls;

use crate::service::{Album, Playlist, Track, TrackStatus};
use serde::{Deserialize, Serialize, Serializer};
use std::{collections::BTreeMap, fmt::Display};
use tracing::{debug, instrument};

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrackListType {
    Album,
    Playlist,
    Track,
    #[default]
    Unknown,
}

impl Display for TrackListType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrackListType::Album => f.write_fmt(format_args!("album")),
            TrackListType::Playlist => f.write_fmt(format_args!("playlist")),
            TrackListType::Track => f.write_fmt(format_args!("track")),
            TrackListType::Unknown => f.write_fmt(format_args!("unknown")),
        }
    }
}

impl From<&str> for TrackListType {
    fn from(tracklist_type: &str) -> Self {
        match tracklist_type {
            "album" => TrackListType::Album,
            "playlist" => TrackListType::Playlist,
            "track" => TrackListType::Track,
            _ => TrackListType::Unknown,
        }
    }
}

fn serialize_btree<S>(queue: &BTreeMap<u32, Track>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let vec_values: Vec<_> = queue.values().collect();
    vec_values.serialize(s)
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackListValue {
    #[serde(serialize_with = "serialize_btree")]
    pub queue: BTreeMap<u32, Track>,
    pub album: Option<Album>,
    pub playlist: Option<Playlist>,
    pub list_type: TrackListType,
}

impl TrackListValue {
    #[instrument]
    pub fn new(queue: Option<&BTreeMap<u32, Track>>) -> TrackListValue {
        TrackListValue {
            queue: queue.unwrap_or(&BTreeMap::new()).clone(),
            album: None,
            playlist: None,
            list_type: TrackListType::Unknown,
        }
    }

    pub fn total(&self) -> u32 {
        if let Some(album) = &self.album {
            album.total_tracks
        } else if let Some(list) = &self.playlist {
            list.tracks_count
        } else {
            self.queue.len() as u32
        }
    }

    #[instrument(skip(self))]
    pub fn clear(&mut self) {
        self.list_type = TrackListType::Unknown;
        self.album = None;
        self.playlist = None;
        self.queue.clear();
    }

    #[instrument(skip(self, album), fields(album_id = album.id))]
    pub fn set_album(&mut self, album: Album) {
        debug!("setting tracklist album");
        self.album = Some(album);
        debug!("setting tracklist list type");
        self.list_type = TrackListType::Album;
    }

    #[instrument(skip(self))]
    pub fn get_album(&self) -> Option<&Album> {
        if let Some(c) = self.current_track() {
            if let Some(album) = &c.album {
                Some(album)
            } else {
                self.album.as_ref()
            }
        } else {
            self.album.as_ref()
        }
    }

    #[instrument(skip(self))]
    pub fn set_playlist(&mut self, playlist: Playlist) {
        self.playlist = Some(playlist);
        self.list_type = TrackListType::Playlist;
    }

    #[instrument(skip(self))]
    pub fn get_playlist(&self) -> Option<&Playlist> {
        self.playlist.as_ref()
    }

    #[instrument(skip(self))]
    pub fn set_list_type(&mut self, list_type: TrackListType) {
        self.list_type = list_type;
    }

    #[instrument(skip(self))]
    pub fn list_type(&self) -> &TrackListType {
        &self.list_type
    }

    #[instrument(skip(self))]
    pub fn find_track_by_index(&self, index: u32) -> Option<&Track> {
        self.queue.get(&index)
    }

    #[instrument(skip(self))]
    pub fn set_track_status(&mut self, position: u32, status: TrackStatus) {
        if let Some(track) = self.queue.get_mut(&position) {
            track.status = status;
        }
    }

    #[instrument(skip(self))]
    pub fn all_tracks(&self) -> Vec<&Track> {
        self.queue.values().collect::<Vec<&Track>>()
    }

    #[instrument(skip(self))]
    pub fn unplayed_tracks(&self) -> Vec<&Track> {
        self.queue
            .iter()
            .filter_map(|t| {
                if t.1.status == TrackStatus::Unplayed {
                    Some(t.1)
                } else {
                    None
                }
            })
            .collect::<Vec<&Track>>()
    }

    #[instrument(skip(self))]
    pub fn played_tracks(&self) -> Vec<&Track> {
        self.queue
            .iter()
            .filter_map(|t| {
                if t.1.status == TrackStatus::Played {
                    Some(t.1)
                } else {
                    None
                }
            })
            .collect::<Vec<&Track>>()
    }

    #[instrument(skip(self))]
    pub fn track_index(&self, track_id: u32) -> Option<u32> {
        let mut index: Option<u32> = None;

        self.queue.iter().for_each(|(i, t)| {
            if t.id == track_id {
                index = Some(*i);
            }
        });

        index
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.queue
            .values()
            .find(|&track| track.status == TrackStatus::Playing)
    }

    pub fn cursive_list(&self) -> Vec<(&str, i32)> {
        self.queue
            .values()
            .map(|i| (i.title.as_str(), i.id as i32))
            .collect::<Vec<(&str, i32)>>()
    }
}
