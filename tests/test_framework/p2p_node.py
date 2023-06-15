import os
import socket
import struct
import time
import hashlib
from typing import Optional

NODE_NETWORK = 1


class P2PMessage():
    """ A class to represent a P2P message. A p2p message is a message that is sent between nodes in the bitcoin network.
    It has a command and a payload. The command is a string that represents the type of message. The payload is the data that
    is sent with the message, and a double-sha checksum.
    """

    def __init__(self, command, payload):
        self.command = command
        self.payload = payload

    def __repr__(self):
        return f"P2PMessage({self.command}, {self.payload})"

    def __str__(self):
        return f"P2PMessage({self.command}, {self.payload})"

    def serialize(self):
        """Serializes the message into a byte string that can be sent over the network.
        """
        magic = 0xD9B4BEF9
        command = self.command.encode(
            'utf-8') + b'\x00' * (12 - len(self.command))
        length = len(self.payload)
        checksum = hashlib.sha256(hashlib.sha256(
            self.payload).digest()).digest()[:4]
        return struct.pack('<L12sL4s', magic, command, length, checksum) + self.payload

    @classmethod
    def deserialize(cls, data):
        """Deserializes a byte string into a P2PMessage object.
        """
        magic, command, length, checksum = struct.unpack('<L12sL4s', data[:24])
        command = command.strip(b'\x00').decode('utf-8')
        payload = data[24:24 + length]
        return cls(command, payload)

    @classmethod
    def create_version(cls):
        """A version message is sent between nodes at the beginning of a connection. It contains information about the node
        that is sending the message, like suported services, the timestamp, and the user agent.
        """
        version = 70016
        services = NODE_NETWORK
        timestamp = int(time.time())
        addr_recv = struct.pack('<Q16s', services, b'\x00' * 16)
        addr_from = struct.pack('<Q16s', services, b'\x00' * 16)
        nonce = 0
        user_agent = b'1'
        user_agent_length = len(user_agent)
        user_agent_length_compact = user_agent_length.to_bytes(
            (user_agent_length.bit_length() + 7) // 8, 'little')
        start_height = 0
        relay = 1
        payload = struct.pack('<LQQ26s26sQ', version, services, timestamp, addr_recv, addr_from,
                              nonce) + user_agent_length_compact + user_agent + struct.pack('<L?', start_height, relay)
        return cls("version", payload)

    @classmethod
    def create_verack(cls):
        """A verack message is sent after a version message is received. It is a simple message that acknowledges that the
        version message was received and the remote node agrees with the provided information.
        """
        return cls("verack", b'')

    @classmethod
    def create_getheaders(cls, start_hash, stop_hash):
        version = 70016
        num_hashes = 1
        return cls("getheaders", struct.pack('<L', version) + struct.pack('<B', num_hashes) + start_hash + stop_hash)

    @classmethod
    def create_getdata(cls, inventory):
        count = len(inventory)
        return cls("getdata", struct.pack('<B', count) + inventory)

    @classmethod
    def create_getblocks(cls, start_hash, stop_hash):
        version = 70016
        num_hashes = 1
        return cls("getblocks", struct.pack('<L', version) + struct.pack('<B', num_hashes) + start_hash + stop_hash)

    @classmethod
    def create_headers(cls, headers):
        num_headers = len(headers)
        return cls("headers", struct.pack('<B', num_headers) + b''.join(headers))

    @classmethod
    def create_block(cls, block):
        return cls("block", block)


class P2PConnection():
    def __init__(self, addr: (str, int)):
        self.addr = addr
        self.sock = socket.create_connection(addr)
        self.messages = []

    def send(self, message: P2PMessage):
        self.sock.sendall(message.serialize())

    def has_message(self, command: str) -> Optional[P2PMessage]:
        for message in self.messages:
            if message.command == command:
                return message
        return None

    def receive(self):

        incoming = self.sock.recv(24)
        magic, command, length, checksum = struct.unpack(
            '<L12sL4s', incoming)
        if magic != 0xD9B4BEF9:
            raise RuntimeError("Magic is wrong")
        payload = self.sock.recv(length)
        self.messages.append(P2PMessage.deserialize(incoming + payload))

    def pop_message(self):
        return self.messages.pop(0)

    def close(self):
        pass


class Node():
    def __init__(self):
        self.connections = []

    def create_connection(self, addr):
        connection = P2PConnection(addr)
        self.connections.append(connection)
        return connection


if __name__ == '__main__':
    node = Node()
    connection = node.create_connection(("192.168.42.2", 8333))
    connection.send(P2PMessage.create_version())
    connection.send(P2PMessage.create_verack())
    connection.receive()
    print(connection.has_message("version"))
