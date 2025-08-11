## WebRTC File Transfer Protocol

### 1. Initiating a File Transfer

Only the **client** can initiate a file transfer.
The client **always** opens a file dialog first — regardless of whether the transfer will be client-to-server or server-to-client.

The client then sends:

```json
{
    "type": "requesttransfer",
    "id": <monotonically increasing integer ID>,
    "size": <optional file size in bytes>
}
```

* **`id`**: A unique, monotonically increasing integer identifying this transfer.
* **`size`** (optional):

  * **Omitted** → Client requests a **download** from the server.
  * **Present** → Client will **upload** a file to the server. The value is the total file size in bytes.

---

### 2. Server Response

* **If client requested a download** (`size` omitted):
  The server prompts the user to select a file.
  Once chosen, the server responds:

  ```json
  {
      "type": "transferready",
      "id": <same ID from requesttransfer>,
      "size": <file size in bytes>
  }
  ```

* **If client is uploading** (`size` present):
  The server responds immediately:

  ```json
  {
      "type": "transferready",
      "id": <same ID from requesttransfer>,
      "size": <file size in bytes>
  }
  ```

---

### 3. Sending File Data

File chunks are sent over the **`ordered-input`** WebRTC data channel as binary packets.

Each packet consists of:

1. **32-bit big-endian integer**: The transfer ID.
2. **Raw file bytes**: A chunk of the file’s content.

* **Upload**: Client sends packets to server.
* **Download**: Server sends packets to client.

---

### 4. Canceling a Transfer

Either side may cancel a transfer at any point — even before `transferready` is sent — by sending:

```json
{
    "type": "canceltransfer",
    "id": <transfer ID>
}
```

**Behavior:**

* All transmission for that ID stops immediately.
* Any open file dialogs for that transfer should be dismissed.
* Both sides must clean up transfer-related state.

**Examples:**

* Server user clicks “Cancel” in their file picker.
* A transfer is aborted midway through data transmission.
