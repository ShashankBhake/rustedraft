# Raft Consensus Algorithm Implementation

## Run Raft

To run the Raft project, use the following command:

```
cargo run <NODE_PORT>
```

> Note: You can use the `-q` flag for quiet mode in cargo.

### Sample Request body for `/append_entries`

```json
{
    "term": 1,
    "entry_index": 0,
    "prev_log_term": 0,
    "entry": {
        "cmd": "SET 1 1",
        "term": 1
    },
    "leader_commit": 0,
    "leader_id": 8001
}
```

### Request with command string to `/execute_command`

To execute a command, send a request with the command string enclosed in double quotes:

```json
"your_command_here"
```

## Supported Log Operations

The Raft project supports the following log operations:

-   Arithmetic log syntax: `Operation result_index operand_1_Index operand_2_Index`

### Commands

-   `SET index_number value`
-   `ADD result_index operand_1_Index operand_2_Index`
-   `SUB result_index operand_1_Index operand_2_Index`
-   `MUL result_index operand_1_Index operand_2_Index`
-   `DIV result_index operand_1_Index operand_2_Index`

Perform an action on the value stored in `operand_1_Index` with `operand_2_Index` and store the result in `result_index`.

Representation example: `state[result_index] = state[operand_1_Index] operation state[operand_2_Index]`

### Examples

#### Add Operation

**Command:** `ADD 2 1 3`

**Initial state:** `[1, -2, 2, 0, 6]`

**Operation:** `state[2] = state[1] + state[3]`

**Final state:** `[1, -2, -2, 0, 6]`

#### Multiply Operation

**Command:** `MUL 2 2 4`

**Initial state:** `[1, -2, -2, 0, 6]`

**Operation:** `state[2] = state[2] * state[4]`

**Final state:** `[1, -2, -12, 0, 6]`
