To begin a FILE TRANSFER:
Client sends:
```json
{
    "type": "requesttransfer",
    "id": <monotonically increasing integer ID>
}
```

Server opens a file dialog to choose a file.

Upon file choice, server responds:
```json
{
    "type": "transferready",
    "id": <previous transfer ID>,
    "size": <file size in bytes>
}
```

Server begins sending BINARY PACKETS consisting ONLY of 32-bit big endian ID prefixed file content down `ordered-input`

If the CLIENT request has a "size," the CLIENT shall send the file.
