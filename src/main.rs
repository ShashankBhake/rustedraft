mod raft_node;

use crate::raft_node::*;
use rand::Rng;
use std::env;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time;
use warp::{reply, Filter, Rejection, Reply};

const URL: &str = "http://localhost";

async fn request_vote(
    body: RequestVoteBody,
    tx: mpsc::Sender<u32>,
    state: Arc<Mutex<RaftNode>>,
) -> Result<impl Reply, Rejection> {
    let mut current_term = state.lock().unwrap().current_term;

    if body.term < current_term {
        return Ok(reply::json(&RequestVoteResponse {
            term: body.term,
            vote_granted: false,
        }));
    }
    let _ = tx.send(3).await;
    current_term = std::cmp::max(current_term, body.term);
    state.lock().unwrap().current_term = current_term;
    Ok(reply::json(&RequestVoteResponse {
        term: current_term,
        vote_granted: true,
    }))
}

async fn append_entries(
    body: AppendEntriesRequest,
    timer_tx: mpsc::Sender<u32>,
    state: Arc<Mutex<RaftNode>>,
) -> Result<impl Reply, Rejection> {
    // println!("ok but role is: {:?}", state.lock().unwrap().role);

    let current_term = state.lock().unwrap().current_term;
    let commit_length = state.lock().unwrap().commit_length;

    let _ = timer_tx.send(0).await;

    if let None = body.entry {
        println!("Heartbeat Received from id: {:?}", body.leader_id);
        return Ok(reply::json(&AppendEntriesResponse {
            term: current_term,
            success: false,
            commit: commit_length,
        }));
    }
    if let Some(entry) = body.entry {
        if body.term < current_term {
            return Ok(reply::json(&AppendEntriesResponse {
                term: current_term,
                success: false,
                commit: commit_length,
            }));
        }
        println!("Log entries: {:?}", entry);
        println!("State before append: {:?}", state.lock().unwrap().vm_state);
        // println!(
        //     "Entry index: {} Log len: {} committed upto: {}",
        //     body.entry_index,
        //     node.log.len(),
        //     commit_length
        // );
        let mut node = state.lock().unwrap();
        let mut log_len = node.log.len();
        if body.entry_index > log_len {
            return Ok(reply::json(&AppendEntriesResponse {
                term: current_term,
                success: false,
                commit: commit_length,
            }));
        }
        node.log.truncate(body.entry_index);
        node.log.push(entry.clone());
        node.commit_length = node.log.len();
        drop(node);
        vm_execute(&entry.cmd, state.clone());
        if state.lock().unwrap().role == Role::Candidate {
            state.lock().unwrap().role = Role::Follower;
        }
        println!("State after append: {:?}", state.lock().unwrap().vm_state);
    }

    Ok(reply::json(&AppendEntriesResponse {
        term: current_term,
        success: true,
        commit: commit_length,
    }))
}

fn vm_execute(body: &str, state: Arc<Mutex<RaftNode>>) -> bool {
    let body_arr: Vec<_> = body.trim().split_whitespace().collect();
    if body_arr.len() < 3 {
        return false;
    }
    let cmd = body_arr[0];
    let mut node = state.lock().unwrap();
    let vm_len = node.vm_state.len();
    let target_index = body_arr[1].parse().unwrap_or(vm_len);

    let op1: i32 = body_arr[2].parse().unwrap_or(vm_len as i32);
    let op2 = body_arr.get(3).unwrap_or(&"0").parse().unwrap_or(vm_len);

    match cmd {
        "SET" => node.vm_state[target_index] = op1,
        "ADD" => node.vm_state[target_index] = node.vm_state[op1 as usize] + node.vm_state[op2],
        "SUB" => node.vm_state[target_index] = node.vm_state[op1 as usize] - node.vm_state[op2],
        "MUL" => node.vm_state[target_index] = node.vm_state[op1 as usize] * node.vm_state[op2],
        _ => return false,
    }

    // println!("VM State: {:?}", node.vm_state);
    true
}

async fn execute_command(
    body: String,
    state: Arc<Mutex<RaftNode>>,
) -> Result<impl Reply, Rejection> {
    if !vm_execute(&body, state.clone()) {
        return Ok("Invalid command");
    }
    let RaftNode {
        current_term,
        peers,
        commit_length,
        id,
        ..
    } = state.lock().unwrap().clone();
    let client = reqwest::Client::new();
    let entry = LogEntry {
        cmd: body,
        term: current_term,
    };
    state.lock().unwrap().log.push(entry.clone());
    let log = state.lock().unwrap().log.clone();
    let mut okays = 0;
    for i in &peers {
        let resp = client
            .post(format!("{}:{}/append_entries", URL, i))
            .json(&AppendEntriesRequest {
                term: current_term,
                entry_index: log.len() - 1,
                prev_log_term: 0,
                entry: Some(log.last().unwrap().clone()),
                leader_commit: commit_length,
                leader_id: id,
            })
            .send()
            .await;

        let resp = match resp {
            Err(_) => continue,
            Ok(json) => json.json::<AppendEntriesResponse>().await.unwrap(),
        };

        if resp.term > current_term {
            state.lock().unwrap().role = Role::Follower;
            return Ok("Stepping down from leader");
        }
        if resp.success {
            okays += 1
        };
    }
    let majority_mark = 2; //(node.peers.len() as f64 / 2.0).floor();
    let mut node = state.lock().unwrap();
    if okays >= majority_mark as u32 {
        node.commit_length += 1;
    }

    if node.commit_length != node.log.len() {
        node.log.truncate(commit_length);
        return Ok("Replication Failed");
    }
    Ok("Replicated!")
}

async fn catch_up(starting_at: usize, state: Arc<Mutex<RaftNode>>) {
    let client = reqwest::Client::new();
    let RaftNode {
        log,
        peers,
        commit_length,
        current_term,
        id,
        ..
    } = state.lock().unwrap().clone();
    println!("Syncing from: {starting_at}");
    for i in starting_at..commit_length {
        for p in &peers {
            let response = client
                .post(format!("{}:{}/append_entries", URL, p))
                .json(&AppendEntriesRequest {
                    term: current_term,
                    entry_index: i,
                    prev_log_term: 0,
                    entry: Some(log[i].clone()),
                    leader_commit: commit_length,
                    leader_id: id,
                })
                .send()
                .await;

            if let Err(e) = response {
                eprintln!("Failed to send append_entries request to {}: {}", p, e);
            }
        }
    }
    println!(
        "My Log length: {}, commit_len: {}",
        log.len(),
        commit_length
    );
}
async fn timer(mut rx: mpsc::Receiver<u32>, state: Arc<Mutex<RaftNode>>) {
    let duration = rand::thread_rng().gen_range(1500..=3000);
    let mut timer = time::interval(Duration::from_millis(duration));
    timer.tick().await;
    loop {
        tokio::select! {
            _ = rx.recv() => {
            timer.reset();
            },
            _ = timer.tick() => {
            println!("Timeout, becoming candidate and starting election");
            election(state.clone()).await;
            },
        }
    }
}

async fn election(state: Arc<Mutex<RaftNode>>) {
    state.lock().unwrap().current_term += 1;
    let RaftNode {
        id,
        current_term,
        log,
        peers,
        ..
    } = state.lock().unwrap().clone();
    println!("{}: Election started, term: {}", id, current_term);

    let client = reqwest::Client::new();
    let mut votes = 0;
    for i in peers {
        let resp = client
            .post(format!("{}:{}/request_vote", URL, i))
            .json(&RequestVoteBody {
                id: id,
                term: current_term,
                log_length: log.len(),
                log_term_last: log.last().unwrap_or(&LogEntry::default()).term,
            })
            .send()
            .await;
        let voted = match resp {
            Err(_) => continue,
            Ok(json) => {
                json.json::<RequestVoteResponse>()
                    .await
                    .unwrap()
                    .vote_granted
            }
        };

        if voted {
            votes += 1
        };
    }

    let majority_mark = 2;
    if votes >= majority_mark as u32 {
        state.lock().unwrap().role = Role::Leader;
        send_heartbeats(state.clone()).await;
    }
}

async fn send_heartbeats(state: Arc<Mutex<RaftNode>>) {
    let mut beat_interval = time::interval(Duration::from_secs(1));
    beat_interval.tick().await;
    let client = reqwest::Client::new();
    let RaftNode {
        mut role,
        peers,
        id,
        ..
    } = state.lock().unwrap().clone();

    while role == Role::Leader {
        let RaftNode {
            current_term,
            log,
            commit_length,
            ..
        } = state.lock().unwrap().clone();
        let mut min_commit_len = commit_length;
        for i in &peers {
            let resp = client
                .post(format!("{}:{}/append_entries", URL, i))
                .json(&AppendEntriesRequest {
                    term: current_term,
                    entry_index: log.len(),
                    prev_log_term: log.last().unwrap_or(&LogEntry::default()).term,
                    entry: None,
                    leader_commit: commit_length,
                    leader_id: id,
                })
                .send()
                .await;
            let commit = match resp {
                Err(_) => continue,
                Ok(json) => json.json::<AppendEntriesResponse>().await.unwrap().commit,
            };
            min_commit_len = std::cmp::min(min_commit_len, commit);
        }

        if min_commit_len < commit_length {
            let starting_at = if min_commit_len != 0 {
                min_commit_len - 1
            } else {
                min_commit_len
            };
            catch_up(starting_at, state.clone()).await;
        }

        println!("Sent Heartbeat");
        beat_interval.tick().await;
        role = state.lock().unwrap().role.clone();
    }
}

#[tokio::main]
async fn main() {
    let id = env::args().nth(1).unwrap().parse().unwrap();
    let peers = (8001..=8005)
        .filter_map(|peer_id| if peer_id != id { Some(peer_id) } else { None })
        .collect();
    println!("Peers: {peers:?}");

    let state = Arc::new(Mutex::new(RaftNode::new(id, peers)));

    let st1 = Arc::clone(&state);
    let st2 = Arc::clone(&state);
    let st3 = Arc::clone(&state);

    let (tx, rx) = mpsc::channel(12);
    let tx1 = tx.clone();

    let routes = warp::path("append_entries")
        .and(warp::post())
        .and(warp::body::json())
        .and(warp::any().map(move || tx.clone()))
        .and(warp::any().map(move || state.clone()))
        .and_then(append_entries)
        .or(warp::path("request_vote")
            .and(warp::post())
            .and(warp::body::json())
            .and(warp::any().map(move || tx1.clone()))
            .and(warp::any().map(move || st1.clone()))
            .and_then(request_vote))
        .or(warp::path("execute_command")
            .and(warp::post())
            .and(warp::body::json())
            .and(warp::any().map(move || st2.clone()))
            .and_then(execute_command));

    let server = warp::serve(routes).run(([127, 0, 0, 1], id as u16));

    tokio::join!(server, timer(rx, st3.clone()));
}
