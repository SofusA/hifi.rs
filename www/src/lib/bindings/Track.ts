// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.
import type { Album } from "./Album";
import type { Artist } from "./Artist";
import type { TrackStatus } from "./TrackStatus";

export type Track = { id: number, number: number, title: string, album: Album | null, artist: Artist | null, durationSeconds: number, explicit: boolean, hiresAvailable: boolean, samplingRate: number, bitDepth: number, status: TrackStatus, available: boolean, coverArt: string | null, position: number, mediaNumber: number, };