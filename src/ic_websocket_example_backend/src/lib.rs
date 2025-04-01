use ic_cdk::{init, post_upgrade, query, update, caller};

// Import the WebRtcMessage type and functions from canister
use canister::{
    on_close, on_message, on_open, 
    WebRtcMessage,
    create_video_room as canister_create_room,
    get_video_rooms, get_room_count, get_user_current_room
};

use ic_websocket_cdk::{
    CanisterWsCloseArguments, CanisterWsCloseResult, CanisterWsGetMessagesArguments,
    CanisterWsGetMessagesResult, CanisterWsMessageArguments, CanisterWsMessageResult,
    CanisterWsOpenArguments, CanisterWsOpenResult, WsHandlers, WsInitParams,
};

mod canister;

#[init]
fn init() {
    let handlers = WsHandlers {
        on_open: Some(on_open),
        on_message: Some(on_message),
        on_close: Some(on_close),
    };

    let params = WsInitParams::new(handlers);

    ic_websocket_cdk::init(params);
}

#[post_upgrade]
fn post_upgrade() {
    init();
}

// WebSocket protocol methods
#[update]
fn ws_open(args: CanisterWsOpenArguments) -> CanisterWsOpenResult {
    ic_websocket_cdk::ws_open(args)
}

#[update]
fn ws_close(args: CanisterWsCloseArguments) -> CanisterWsCloseResult {
    ic_websocket_cdk::ws_close(args)
}

// IMPORTANT: Updated to accept WebRtcMessage instead of AppMessage
#[update]
fn ws_message(args: CanisterWsMessageArguments) -> CanisterWsMessageResult {
    ic_websocket_cdk::ws_message(args, Some(caller()))
}

#[query]
fn ws_get_messages(args: CanisterWsGetMessagesArguments) -> CanisterWsGetMessagesResult {
    ic_websocket_cdk::ws_get_messages(args)
}

// Video room management methods
#[update]
pub fn create_video_room(room_id: String) -> String {
    canister_create_room(room_id)
}

#[query]
pub fn list_video_rooms() -> Vec<String> {
    get_video_rooms()
}

#[query]
pub fn get_room_participant_count(room_id: String) -> usize {
    get_room_count(room_id)
}

#[query]
pub fn get_current_room() -> Option<String> {
    get_user_current_room(caller())
}

// For candid interface generation
ic_cdk::export_candid!();