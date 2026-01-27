# **MCS Protocol Definition**

The MCS protocol uses a simple binary framing format. Each message consists of a 5-byte header followed by a variable-length payload. All integers are encoded as Big-Endian.

## **Frame Structure**

| Field | Size | Description |
| :---- | :---- | :---- |
| Type | 1 byte | Message identifier (see below) |
| Length | 4 bytes | Size of the payload in bytes |
| Payload | N bytes | Variable message data |

## **Message Types**

### **Chat (0x01)**

Used for sending and receiving chat messages.

**Payload Layout:**

1. **Timestamp** (i64, 8 bytes): Unix timestamp.  
2. **Sender Length** (u32, 4 bytes): Length of the sender's username.  
3. **Sender** (Bytes): UTF-8 string of the sender's name.  
4. **Content** (Bytes): UTF-8 string (remaining bytes in payload).

### **Join (0x02)**

Sent by the client to register a username.

**Payload Layout:**

1. **Username** (Bytes): UTF-8 string (entire payload).

### **Heartbeat (0x03)**

Keep-alive signal exchanged between client and server.

**Payload Layout:**

* Empty (Length is 0).

### **Error (0x04)**

Sent when an operation fails (e.g., username taken).

**Payload Layout:**

1. **Serialized ChatError** (Bytes): A `ChatError` enum serialized using [Postcard](https://github.com/jamesmunns/postcard).

**ChatError Variants:**
* `Network`
* `UsernameTaken`
* `UsernameTooShort`
* `Internal`
