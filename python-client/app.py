# %%
from websockets.sync.client import connect
from dataclasses import dataclass
import socket
import struct
from time import perf_counter_ns
import time
from typing import Dict
from matplotlib.axes import Axes
from matplotlib.gridspec import GridSpec
import numpy as np
import pandas as pd
import matplotlib.pyplot as plt
import json
# %%
DPI = 72
fig_wid = 1920 / DPI
fig_hei = 1080 / DPI
plt.ioff()
grid = GridSpec(2, 2, width_ratios=[1, 1], height_ratios=[1, 1])
fig = plt.figure(figsize=(fig_wid, fig_hei), dpi=DPI)
axs = []
for i in range(2):
    axs.append([])
    for j in range(2):
        if i > 0:
            ax = fig.add_subplot(grid[i, j], sharex=axs[0][j])
        else:
            ax = fig.add_subplot(grid[i, j])
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
    ax.set_xlim(-1000, 0)
    ax.set_xlabel('Offset (ms)', fontsize=10)


fig.suptitle("Accelerometer Data", fontsize=12, fontweight='bold')
fig.tight_layout()
ids = []
last_update = None
fig.show()
fignum = fig.number
# %%
client = connect("ws://localhost:14389")

dataframes: Dict[int, pd.DataFrame] = dict()
packets: Dict[int, int] = dict()  # For debugging purposes

start = perf_counter_ns()
while True:
    try:
        data = client.recv()
        if isinstance(data, str):
            try:
                datas = json.loads(data)
            except json.JSONDecodeError:
                print(f"Received non-JSON data: {data}")
                continue
            try:
                for data in datas:
                    try:
                        id = data['idx']
                        gap = data['gap']
                        x = data['x']
                        y = data['y']
                        z = data['z']
                    except KeyError as e:
                        print(f"Missing key in data: {e}, {data}")
                        continue
                    if id not in dataframes:
                        print(
                            f"Creating new DataFrame: {id}, {gap}, {x}, {y}, {z}")
                        ids.append(id)
                        dataframes[id] = pd.DataFrame({
                            'tstamp': pd.Series(dtype=int),
                            'x': pd.Series(dtype='float'),
                            'y': pd.Series(dtype='float'),
                            'z': pd.Series(dtype='float'),
                            'dx': pd.Series(dtype='float'),
                            'dy': pd.Series(dtype='float'),
                            'dz': pd.Series(dtype='float'),
                        })
                        dataframes[id].loc[0] = [gap, x, y, z,
                                                 np.nan, np.nan, np.nan]  # adding a row
                        packets[id] = 1
                    else:
                        dflen = len(dataframes[id])
                        last_row = dataframes[id].iloc[dflen - 1]
                        tstamp = last_row['tstamp'] + gap
                        gap *= 1e-6  # Convert to seconds
                        dx = (x - last_row['x']) / gap
                        dy = (y - last_row['y']) / gap
                        dz = (z - last_row['z']) / gap
                        dataframes[id].loc[dflen] = [tstamp, x,
                                                  y, z, dx, dy, dz]  # adding a row
                        packets[id] += 1
            except Exception as e:
                print(f"Error processing data: {e}, {datas}")
                continue
    except Exception as e:
        print(f"Error: {e}")
        client.close()
        break
    except KeyboardInterrupt:
        print("Interrupted by user")
        client.close()
        plt.close(fig)
        break
    if last_update is None:
        last_update = perf_counter_ns()
    elif perf_counter_ns() - last_update > 16e6:  # Update every 16 ms
        # last_update = perf_counter_ns()
        # print(
        #     f'Received {", ".join(f"{id}: {packets[id]}" for id in ids if id in packets)} packets.')
        # for id in ids:
        #     packets[id] = 0  # Reset packet count for next interval
        for (axm, id) in zip(axs, ids):
            if id in dataframes:
                df = dataframes[id]
                if df.empty:
                    print(f"ID {id}> No data available")
                else:
                    print(f"ID {id}> {len(df)} total points")
                now = df['tstamp'].iloc[-1]
                sel = df['tstamp'] > (now - 1e6)  # Show last second of data
                tstamp = df['tstamp'][sel]  # Convert to milliseconds
                tstamp = tstamp * 1e-6  # Convert to seconds
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
                print(f"ID {id}> {len(tstamp)} points, time range {tstamp.iloc[0]} to {tstamp.iloc[-1]} ms")
                print(f'ID {id}> Y-axis limits: Y: ({ymin}, {ymax}), dY: ({dymin}, {dymax})')
                for aid, ax in enumerate(axm):
                    ax.clear()
                    if aid == 0:
                        ax.plot(tstamp, df['x'][sel], label='X', color='red')
                        ax.plot(tstamp, df['y'][sel], label='Y', color='green')
                        ax.plot(tstamp, df['z'][sel], label='Z', color='blue')
                        # ax.set_ylim(ymin, ymax)
                        ax.set_ylim(-2, 2)
                    elif aid == 1:
                        ax.plot(tstamp, df['dx'][sel], label='dX', color='red')
                        ax.plot(tstamp, df['dy'][sel],
                                label='dY', color='green')
                        ax.plot(tstamp, df['dz'][sel],
                                label='dZ', color='blue')
                        ax.set_ylim(dymin, dymax)
                    ax.set_xlim(-1000, 0)

        fig.canvas.draw()
        fig.canvas.flush_events()

    if plt.fignum_exists(fignum) is False:
        print("Figure closed, exiting loop")
        client.close()
        break
    # if perf_counter_ns() - start > 10e9:  # Stop after 10 seconds
    #     print("Stopping after 10 seconds")
    #     client.close()
    #     break
print("Done receiving data")

# %%
