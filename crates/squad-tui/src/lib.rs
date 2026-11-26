//! Squad terminal UI built on Ratatui.
pub mod actions;
pub mod app;
pub mod clipboard;
pub mod composed;
pub mod event_loop;
pub mod fx;
pub mod input_events;
pub mod layout;
pub mod markdown;
pub mod quotes;
pub mod screens;
pub mod search;
pub mod selection;
pub mod session;
pub mod startup;
pub mod terminal;
pub mod text;
pub mod theme;
pub mod transcript;
pub mod ui;
mod ui_fx;
pub mod validation;
pub mod widgets;

pub mod room_adapter;
pub use room_adapter::run;

pub mod typeahead;
