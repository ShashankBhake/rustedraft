use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq)]
pub enum Role {
    Follower,
    Candidate,
    Leader,
}

#[derive(Clone, Debug)]
pub struct RaftNode {
    pub id: u32,
    pub current_term: u32,
    pub role: Role,
    pub log: Vec<LogEntry>,
    pub commit_length: usize,
    pub peers: Vec<u32>,
    pub next_index: usize,
    pub vm_state: Vec<i32>,
}

impl RaftNode {
    pub fn new(id: u32, peers: Vec<u32>) -> RaftNode {
        RaftNode {
            id: id,
            current_term: 0,
            role: Role::Follower,
            log: Vec::new(),
            commit_length: 0,
            next_index: 0,
            vm_state: vec![0; 5],
            peers,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LogEntry {
    pub cmd: String,
    pub term: u32,
}

impl LogEntry {
    pub fn default() -> Self {
        Self {
            cmd: String::new(),
            term: 0,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct RequestVoteBody {
    pub term: u32,
    pub id: u32,
    pub log_length: usize,
    pub log_term_last: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RequestVoteResponse {
    pub term: u32,
    pub vote_granted: bool,
}

#[derive(Serialize, Deserialize)]
pub struct AppendEntriesRequest {
    pub term: u32,
    pub entry_index: usize,
    pub prev_log_term: u32,
    pub entry: Option<LogEntry>,
    pub leader_commit: usize,
    pub leader_id: u32,
}

#[derive(Serialize, Deserialize)]
pub struct AppendEntriesResponse {
    pub term: u32,
    pub success: bool,
    pub commit: usize,
}
