use candid::{decode_one, encode_one, CandidType};
use ic_cdk::{api::time, print};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use ic_websocket_cdk::{
    send, ClientPrincipal, OnCloseCallbackArgs, OnMessageCallbackArgs, OnOpenCallbackArgs,
};

// ICE candidate for WebRTC connection establishment
#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct IceCandidate {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_m_line_index: Option<u32>,
}

// SDP for WebRTC offer/answer exchange
#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct SessionDescription {
    pub type_: String,  // "offer" or "answer"
    pub sdp: String,
}

// Expanded message type to support WebRTC signaling
#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub enum WebRtcMessage {
    // WebRTC signaling
    Offer(SessionDescription),
    Answer(SessionDescription),
    Ice(IceCandidate),
    
    // Room management
    JoinRoom(String),
    LeaveRoom(String),
    
    // Regular chat message (keep existing functionality)
    Chat(AppMessage),
}

// Original AppMessage struct for backwards compatibility
#[derive(CandidType, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct AppMessage {
    pub text: String,
    pub timestamp: u64,
}

impl AppMessage {
    fn candid_serialize(&self) -> Vec<u8> {
        encode_one(&self).unwrap()
    }
}

// Thread-local storage for rooms
thread_local! {
    static ROOMS: RefCell<HashMap<String, HashSet<ClientPrincipal>>> = RefCell::new(HashMap::new());
    static USER_ROOMS: RefCell<HashMap<ClientPrincipal, String>> = RefCell::new(HashMap::new());
}

// WebSocket event handlers
pub fn on_open(args: OnOpenCallbackArgs) {
    let msg = AppMessage {
        text: String::from("Connected to WebRTC video calling service"),
        timestamp: time(),
    };
    send_app_message(args.client_principal, msg);
}

pub fn on_message(args: OnMessageCallbackArgs) {
    // Try to decode as a WebRtcMessage first
    match decode_one::<WebRtcMessage>(&args.message) {
        Ok(webrtc_msg) => handle_webrtc_message(args.client_principal, webrtc_msg, args.message),
        Err(_) => {
            // Fall back to legacy AppMessage handling
            match decode_one::<AppMessage>(&args.message) {
                Ok(app_msg) => {
                    print(format!("Received legacy message: {:?}", app_msg));
                    let new_msg = AppMessage {
                        text: String::from("ping"),
                        timestamp: time(),
                    };
                    send_app_message(args.client_principal, new_msg);
                }
                Err(e) => {
                    print(format!("Failed to decode message: {}", e));
                }
            }
        }
    }
}

pub fn on_close(args: OnCloseCallbackArgs) {
    // When client disconnects, remove them from any rooms they're in
    if let Some(room_id) = get_user_room(args.client_principal) {
        leave_room(&room_id, args.client_principal);
    }
    print(format!("Client {} disconnected", args.client_principal));
}

// WebRTC message handler
fn handle_webrtc_message(client_principal: ClientPrincipal, msg: WebRtcMessage, raw_msg: Vec<u8>) {
    match msg {
        WebRtcMessage::JoinRoom(room_id) => {
            print(format!("User {} joining room {}", client_principal, room_id));
            join_room(&room_id, client_principal);
        }
        WebRtcMessage::LeaveRoom(room_id) => {
            print(format!("User {} leaving room {}", client_principal, room_id));
            leave_room(&room_id, client_principal);
        }
        WebRtcMessage::Offer(_) | WebRtcMessage::Answer(_) | WebRtcMessage::Ice(_) => {
            // For WebRTC signaling messages, forward them to the room participants
            if let Some(room_id) = get_user_room(client_principal) {
                print(format!(
                    "Forwarding WebRTC signaling message from {} in room {}",
                    client_principal, room_id
                ));
                forward_to_room(&room_id, client_principal, raw_msg);
            } else {
                print(format!(
                    "Received WebRTC message from {} but they're not in any room",
                    client_principal
                ));
            }
        }
        WebRtcMessage::Chat(app_msg) => {
            // Handle chat messages similarly
            print(format!("Received chat message: {:?}", app_msg));
            
            if let Some(room_id) = get_user_room(client_principal) {
                // Forward the chat message to everyone in the room
                let response = WebRtcMessage::Chat(AppMessage {
                    text: app_msg.text,
                    timestamp: time(),
                });
                
                let serialized = encode_one(&response).unwrap();
                forward_to_room(&room_id, client_principal, serialized);
            } else {
                // If not in a room, respond directly to the sender
                let response = AppMessage {
                    text: String::from("Please join a room first"),
                    timestamp: time(),
                };
                send_app_message(client_principal, response);
            }
        }
    }
}

// Room management functions
fn create_room(room_id: String) -> String {
    ROOMS.with(|rooms| {
        let mut rooms_mut = rooms.borrow_mut();
        if !rooms_mut.contains_key(&room_id) {
            rooms_mut.insert(room_id.clone(), HashSet::new());
        }
        room_id
    })
}

fn join_room(room_id: &str, client_principal: ClientPrincipal) {
    ROOMS.with(|rooms| {
        let mut rooms_mut = rooms.borrow_mut();
        
        // Create room if it doesn't exist
        if !rooms_mut.contains_key(room_id) {
            rooms_mut.insert(room_id.to_string(), HashSet::new());
        }
        
        // Add participant to room
        if let Some(participants) = rooms_mut.get_mut(room_id) {
            participants.insert(client_principal);
        }
    });
    
    // Associate user with room
    USER_ROOMS.with(|user_rooms| {
        let mut user_rooms_mut = user_rooms.borrow_mut();
        user_rooms_mut.insert(client_principal, room_id.to_string());
    });
    
    // Notify everyone in the room about the new participant
    notify_room_participants(room_id, client_principal, "joined");
}

fn leave_room(room_id: &str, client_principal: ClientPrincipal) {
    ROOMS.with(|rooms| {
        let mut rooms_mut = rooms.borrow_mut();
        if let Some(participants) = rooms_mut.get_mut(room_id) {
            participants.remove(&client_principal);
            
            // Clean up empty rooms
            if participants.is_empty() {
                rooms_mut.remove(room_id);
            }
        }
    });
    
    // Remove user-room association
    USER_ROOMS.with(|user_rooms| {
        let mut user_rooms_mut = user_rooms.borrow_mut();
        user_rooms_mut.remove(&client_principal);
    });
    
    // Notify room participants
    notify_room_participants(room_id, client_principal, "left");
}

fn get_room_participants(room_id: &str) -> Vec<ClientPrincipal> {
    ROOMS.with(|rooms| {
        let rooms_ref = rooms.borrow();
        match rooms_ref.get(room_id) {
            Some(participants) => participants.iter().copied().collect(),
            None => Vec::new(),
        }
    })
}

fn get_user_room(client_principal: ClientPrincipal) -> Option<String> {
    USER_ROOMS.with(|user_rooms| {
        let user_rooms_ref = user_rooms.borrow();
        user_rooms_ref.get(&client_principal).cloned()
    })
}

// Helper functions for notifications and message forwarding
fn notify_room_participants(room_id: &str, sender: ClientPrincipal, event_type: &str) {
    let participants = get_room_participants(room_id);
    let msg = AppMessage {
        text: format!("User {} has {} the room", sender, event_type),
        timestamp: time(),
    };
    
    for participant in participants {
        if participant != sender {
            send_app_message(participant, msg.clone());
        }
    }
}

fn forward_to_room(room_id: &str, sender: ClientPrincipal, message: Vec<u8>) {
    let participants = get_room_participants(room_id);
    
    for participant in participants {
        if participant != sender {
            if let Err(e) = send(participant, message.clone()) {
                print(format!("Failed to forward message to {}: {}", participant, e));
            }
        }
    }
}

fn send_app_message(client_principal: ClientPrincipal, msg: AppMessage) {
    print(format!("Sending message: {:?}", msg));
    if let Err(e) = send(client_principal, msg.candid_serialize()) {
        println!("Could not send message: {}", e);
    }
}

// Exporter functions to be used by lib.rs
pub fn create_video_room(room_id: String) -> String {
    create_room(room_id)
}

pub fn get_video_rooms() -> Vec<String> {
    ROOMS.with(|rooms| {
        let rooms_ref = rooms.borrow();
        rooms_ref.keys().cloned().collect()
    })
}

pub fn get_room_count(room_id: String) -> usize {
    ROOMS.with(|rooms| {
        let rooms_ref = rooms.borrow();
        match rooms_ref.get(&room_id) {
            Some(participants) => participants.len(),
            None => 0,
        }
    })
}

pub fn get_user_current_room(principal: ClientPrincipal) -> Option<String> {
    get_user_room(principal)
}