from __future__ import annotations
from collections import deque
import socket
import struct
from time import perf_counter_ns, sleep
from typing import Dict
import numpy as np
import pandas as pd
from queue import Queue
from threading import Thread


class DataBuffer:
    def __init__(self, maxlen=2000):
        # Ensure maxlen is a power of 2
        maxlen = 1 << int(np.ceil(np.log2(maxlen)))
        self._data = deque(maxlen=maxlen)

    def append(self, item):
        self._data.append(item)

    def clear(self):
        self._data.clear()

    def __getitem__(self, index):
        return self._data[index]

    def __len__(self):
        return len(self._data)

    def to_dataframe(self, columns=None):
        if columns is None:
            columns = ['tstamp', 'x', 'y', 'z', 'dx', 'dy', 'dz']
        return pd.DataFrame(list(self._data), columns=columns)


class DataRate:
    def __init__(self, update_rate: float = 2.0, text: str = "Data rate", bps: bool = True):
        self.count = 0
        self.last = None  # Last timestamp for calculating data rate
        self.update_rate = update_rate
        self.text = text
        self.bps = bps

    def update(self, num_samples: int = 1):
        now = perf_counter_ns()
        self.count += num_samples
        if self.last is None:
            self.last = now
            return
        elapsed = (now - self.last) / 1e9
        if elapsed > self.update_rate:  # Update every second
            rate = self.count / elapsed
            self.last = now
            self.count = 0
            if self.bps:
                rate *= 8  # Convert to bits per second
                unit = 'bps'
                if rate > 1024:
                    rate /= 1024
                    unit = 'Kbps'
                elif rate > 1024*1024:
                    rate /= 1024*1024
                    unit = 'Mbps'
            else:
                unit = 'samples/s'
                if rate > 1000:
                    rate /= 1000
                    unit = 'Ksamples/s'
            print(f'\t{self.text}: {rate:.2f} {unit}')


class TcpThread(Thread):
    def __init__(self, host: str, port: int, queue: Queue, datasize: int = 2000):
        super().__init__()
        self.host = host
        self.port = port
        self.queue = queue
        self.running = True
        self.datasize = datasize

    def run(self):
        # Use DataBuffer for efficient data handling
        ids = []
        datasets: Dict[int, DataBuffer] = dict()
        packets: Dict[int, int] = dict()  # For debugging purposes
        datarate = DataRate(update_rate=1.0)
        packetrate = DataRate(update_rate=1.0, text="Packet rate", bps=False)
        while True:
            client = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            try:
                client.connect((self.host, self.port))
            except Exception as e:
                sleep(0.1)
                continue
            break
            
        print(f"Connected to {self.host}:{self.port}")
        last = perf_counter_ns()
        while True:
            now = perf_counter_ns()
            try:
                bytes = client.recv(20)
                datarate.update(len(bytes))
                packetrate.update()
                (id, gap, x, y, z) = struct.unpack('<IIfff', bytes)
                gap *= 1e-6 # Convert gap to seconds
                if id not in datasets:
                    print(f"Creating new DataBuffer: {id}, {gap}, {x}, {y}, {z}")
                    ids.append(id)
                    datasets[id] = DataBuffer(maxlen=self.datasize)
                    datasets[id].append((gap, x, y, z, np.nan, np.nan, np.nan))
                    packets[id] = 1
                else:
                    tstamp, x0, y0, z0, _, _, _ = datasets[id][-1]
                    tstamp += gap
                    dx = (x - x0) / gap
                    dy = (y - y0) / gap
                    dz = (z - z0) / gap
                    datasets[id].append((tstamp, x, y, z, dx, dy, dz))
                    packets[id] += 1
                    if now - last > 100e6:  # If more than 100 ms since last update
                        last = now
                        dataframes = [(id, datasets[id].to_dataframe()) for id in ids if len(datasets[id]) > 0]
                        if len(dataframes) > 0:
                            self.queue.put_nowait(dataframes)
            except struct.error as e:
                print(f"Error unpacking data: {e}, received data: {bytes}") # type: ignore
                continue
            except Exception as e:
                print(f"Error: {e}")
                client.close()
                break
            except KeyboardInterrupt:
                print("Interrupted by user")
                client.close()
                break

