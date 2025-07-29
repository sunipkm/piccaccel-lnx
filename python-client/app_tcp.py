# %%
from pathlib import Path
import socket
import struct
from time import perf_counter_ns, sleep, time
from typing import Dict, Optional
from matplotlib.animation import FuncAnimation
from matplotlib.axes import Axes
from matplotlib.gridspec import GridSpec
import numpy as np
import pandas as pd
import matplotlib.pyplot as plt
from collections import deque
from netCDF4 import Dataset
from datetime import datetime
from matplotlib.widgets import Button
import threading
from queue import Queue, Empty
from tcp_thread import TcpThread
from nc_thread import NcDataset
import warnings

warnings.filterwarnings("ignore", category=RuntimeWarning)  # Ignore matplotlib warnings

import matplotlib
matplotlib.use('QtAgg')  # Use TkAgg backend for interactive plotting
# %%


class DataBuffer:
    def __init__(self, maxlen=2000):
        maxlen = 1 << int(np.ceil(np.log2(maxlen)))  # Ensure maxlen is a power of 2
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
    def __init__(self, update_rate: float = 2.0):
        self.count = 0
        self.last = None  # Last timestamp for calculating data rate
        self.update_rate = update_rate

    def update(self, num_samples: int = 1):
        now = perf_counter_ns()
        self.count += num_samples
        if self.last is None:
            self.last = now
            return
        elapsed = (now - self.last) / 1e9
        if elapsed > 2.0:  # Update every second
            rate = self.count * 8 / elapsed
            self.last = now
            self.count = 0
            unit = 'bps'
            if rate > 1024:
                rate /= 1024
                unit = 'Kbps'
            elif rate > 1024*1024:
                rate /= 1024*1024
                unit = 'Mbps'
            print(f'Data rate: {rate:.2f} {unit}')
        

# %%
DPI = 72
FIG_WID = 800 / DPI
FIG_HEI = 600 / DPI


def run(queue: Queue, winsize: int = 1000):
    plt.ioff()
    grid = GridSpec(3, 8, width_ratios=[1]*8, height_ratios=[
                    0.1, 1, 1], left=0.065, bottom=0.065, wspace=0.5)
    fig = plt.figure(figsize=(FIG_WID, FIG_HEI), dpi=DPI, animated=True)
    button_ax = fig.add_subplot(grid[0, 3:5])
    # button_ax.set_axis_off()
    ncfile = NcDataset(Path.cwd() / 'data', button_ax)
    axs = []
    for i in range(2):
        axs.append([])
        for j in range(2):
            ar = slice(j*4, (j+1)*4)
            if i > 0:
                ax = fig.add_subplot(grid[i+1, ar], sharex=axs[0][j])
            else:
                ax = fig.add_subplot(grid[i+1, ar])
            axs[i].append(ax)
    axs = np.asarray(axs)
    for ax in axs[:-1, :].flatten():
        ax: Axes = ax
        ax.xaxis.set_visible(False)
    for ax in axs[:, -1].flatten():
        ax: Axes = ax
        ax.yaxis.tick_right()
    for (ax, title) in zip(axs[0, :].flatten(), ('Acceleration', 'Jerk')):
        ax: Axes = ax
        ax.set_title(title, fontsize=10, fontweight='bold')
    for ax in axs[-1, :].flatten():
        ax: Axes = ax
        ax.set_xlim(-winsize, 0)
        ax.set_xlabel('Offset (ms)', fontsize=10)

    lines = []
    for mid, axm in enumerate(axs):
        lines.append([])
        for aid, ax in enumerate(axm):
            lines[mid].append([])
            if aid % 2 == 0:
                lines[mid][aid].append(ax.plot([], [], label='X', color='red', alpha=0.5)[0])
                lines[mid][aid].append(ax.plot([], [], label='Y', color='green', alpha=0.5)[0])
                lines[mid][aid].append(ax.plot([], [], label='Z', color='blue', alpha=0.5)[0])
            else:
                lines[mid][aid].append(ax.plot([], [], label='dX', color='red', alpha=0.5)[0])
                lines[mid][aid].append(ax.plot([], [], label='dY', color='green', alpha=0.5)[0])
                lines[mid][aid].append(ax.plot([], [], label='dZ', color='blue', alpha=0.5)[0])


    fig.suptitle("Accelerometer Data", fontsize=12, fontweight='bold')
    # fig.tight_layout()
    fig.text(0.025, 0.5, "Acceleration (g)", fontsize=12,
             ha='center', va='center', rotation='vertical')
    fig.text(0.5, 0.025, "Time (ms)", fontsize=12, ha='center', va='center')
    fig.text(0.9725, 0.5, "Jerk (g/s)", fontsize=12,
             ha='center', va='center', rotation='vertical')

    fig.show()

    def update(frame):
        try:
            dataframes = queue.get_nowait()
            ncfile.update(dataframes)
            for (llines, axm, (id, df)) in zip(lines, axs, dataframes):
                df: pd.DataFrame = df
                id: int = id
                if df.empty:
                    print(f"ID {id}> No data available")
                    continue
                now = df['tstamp'].iloc[-1]
                # Show last second of data
                sel = df['tstamp'] > (now - winsize * 1e-3)
                tstamp = df['tstamp'][sel]  # Convert to milliseconds
                # tstamp = tstamp * 1e-6  # Convert to seconds
                tstamp -= tstamp.iloc[-1]
                tstamp *= 1e3  # Convert to milliseconds for plotting
                ymin_x = np.nanmin(df['x'][sel])
                ymax_x = np.nanmax(df['x'][sel])
                ymin_y = np.nanmin(df['y'][sel])
                ymax_y = np.nanmax(df['y'][sel])
                ymin_z = np.nanmin(df['z'][sel])
                ymax_z = np.nanmax(df['z'][sel])
                ymin_dx = np.nanmin(df['dx'][sel]) 
                ymax_dx = np.nanmax(df['dx'][sel])
                ymin_dy = np.nanmin(df['dy'][sel])
                ymax_dy = np.nanmax(df['dy'][sel])
                ymin_dz = np.nanmin(df['dz'][sel])
                ymax_dz = np.nanmax(df['dz'][sel])
                ymin = np.nanmin((ymin_x, ymin_y, ymin_z))
                ymax = np.nanmax((ymax_x, ymax_y, ymax_z))
                dymin = np.nanmin((ymin_dx, ymin_dy, ymin_dz))
                dymax = np.nanmax((ymax_dx, ymax_dy, ymax_dz))
                if np.isnan(ymin):
                    ymin = -1
                if np.isnan(ymax):
                    ymax = 1
                if np.isnan(dymin):
                    dymin = -1
                if np.isnan(dymax):
                    dymax = 1
                # print(f"ID {id}> {len(tstamp)} points, time range {tstamp.iloc[0]} to {tstamp.iloc[-1]} ms")
                # print(f'ID {id}> Y-axis limits: Y: ({ymin}, {ymax}), dY: ({dymin}, {dymax})')
                for aid, (lline, ax) in enumerate(zip(llines, axm)):
                    lline: list = lline
                    ax: Axes = ax
                    if aid % 2 == 0:
                        lline[0].set_data(tstamp, df['x'][sel])
                        lline[1].set_data(tstamp, df['y'][sel])
                        lline[2].set_data(tstamp, df['z'][sel])
                        ax.set_ylim(-2, 2)
                    else:
                        lline[0].set_data(tstamp, df['dx'][sel])
                        lline[1].set_data(tstamp, df['dy'][sel])
                        lline[2].set_data(tstamp, df['dz'][sel])
                        if not (np.isnan(dymin) and np.isnan(dymax)):
                            ax.set_ylim(dymin, dymax)
                    ax.relim()
                    ax.autoscale_view()
                # draw_start = perf_counter_ns()
                # fig.canvas.draw_idle()
                # draw_end = perf_counter_ns()
                # draw_time = (draw_end - draw_start) *1e-6
                # if draw_time > 100:
                #     print(f"\tDrawing took too long: {draw_time:.2f} ms, consider reducing window size")
                # flush_start = perf_counter_ns()
                # fig.canvas.flush_events()
                # flush_end = perf_counter_ns()
                # flush_time = (flush_end - flush_start) * 1e-6
                # if flush_time > 100:
                #     print(f"\tFlushing took too long: {flush_time:.2f} ms")
        except Empty:
            sleep(0.01)
        flatlines = [line for sublist in lines for line in sublist]
        flatlines = [line for sublist in flatlines for line in sublist]
        return flatlines

    animation = FuncAnimation(fig, update, blit=True, repeat=False, save_count=100)
    plt.show()
    print("Done receiving data")
    ncfile.close()
    print("NetCDF file closed")
    plt.ion()


# %%
if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(
        description="TCP Client for Accelerometer Data")
    parser.add_argument(
        'host', type=str, help='Host address of the TCP server', default='localhost', nargs='?')
    parser.add_argument(
        'port', type=int, help='Port number of the TCP server', default=14389, nargs='?')
    parser.add_argument(
        '--window', type=int, default=1, help='Window size for data display in seconds'
    )
    args = parser.parse_args()
    winsize = args.window*1000
    if winsize < 1000:
        print(f"Window size {winsize} ms is too small, setting to 1000 ms")
        winsize = 1000
    elif winsize > 10000:
        print(f"Window size {winsize} ms is too large, setting to 10000 ms")
        winsize = 10000
    queue = Queue()
    thread = TcpThread(args.host, args.port, queue, datasize=winsize)
    thread.daemon = True  # Ensure the thread exits when the main program exits
    thread.start()
    print(f"Starting TCP client thread for {args.host}:{args.port} with window size {winsize} ms")
    run(queue, winsize=winsize)
