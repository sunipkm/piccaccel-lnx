# %%
from collections import deque
from datetime import datetime
from netCDF4 import Dataset
from websockets.sync.client import connect
from time import perf_counter_ns
from typing import Dict
from matplotlib.axes import Axes
from matplotlib.gridspec import GridSpec
import numpy as np
import pandas as pd
import matplotlib.pyplot as plt
import json

import matplotlib
matplotlib.use('QtAgg')  # Use TkAgg backend for interactive plotting
# %%
class DataBuffer:
    def __init__(self, maxlen=2000):
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
# %%
DPI = 72
FIG_WID = 800 / DPI
FIG_HEI = 600 / DPI

def run(addr: str, port: int):
    plt.ioff()
    grid = GridSpec(2, 2, width_ratios=[1, 1], height_ratios=[1, 1], left=0.065, bottom=0.065)
    fig = plt.figure(figsize=(FIG_WID, FIG_HEI), dpi=DPI)
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
    # fig.tight_layout()
    ids = []
    last_update = None
    fig.text(0.025, 0.5, "Acceleration (g)", fontsize=12, ha='center', va='center', rotation='vertical')
    fig.text(0.5, 0.025, "Time (ms)", fontsize=12, ha='center', va='center')
    fig.text(0.9725, 0.5, "Jerk (g/s)", fontsize=12, ha='center', va='center', rotation='vertical')

    fig.show()
    fignum = fig.number
    # %%
    try:
        client = connect(f"ws://{addr}:{port}")
    except Exception as e:
        print(f"Failed to connect to {addr}:{port} - {e}")
        return

    # Use DataBuffer for efficient data handling
    datasets: Dict[int, DataBuffer] = dict()
    packets: Dict[int, int] = dict()  # For debugging purposes
    ncfile = Dataset(f"{datetime.now():%Y%m%d_%H%M%S}.nc",
                     'w', format='NETCDF4')
    ncfile.createDimension('id', None)
    ncid = ncfile.createVariable('id', 'i4', ('id',), zlib=True)
    nctime = ncfile.createVariable('time', 'f8', ('id',), zlib=True)
    ncx = ncfile.createVariable('x', 'f4', ('id',), zlib=True)
    ncy = ncfile.createVariable('y', 'f4', ('id',), zlib=True)
    ncz = ncfile.createVariable('z', 'f4', ('id',), zlib=True)

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
                        if id not in datasets:
                            print(f"Creating new DataBuffer: {id}, {gap}, {x}, {y}, {z}")
                            ids.append(id)
                            datasets[id] = DataBuffer(maxlen=2000)
                            datasets[id].append((gap, x, y, z, np.nan, np.nan, np.nan))
                            packets[id] = 1
                        else:
                            tstamp, x0, y0, z0, _, _, _ = datasets[id][-1]
                            tstamp += gap
                            gap *= 1e-6  # Convert to seconds
                            dx = (x - x0) / gap
                            dy = (y - y0) / gap
                            dz = (z - z0) / gap
                            datasets[id].append((tstamp, x, y, z, dx, dy, dz))
                            packets[id] += 1
                except Exception as e:
                    print(f"Error processing data: {e}, {datas}")
                    continue
            
            if last_update is None:
                last_update = perf_counter_ns()
            elif perf_counter_ns() - last_update > 100e6:  # Update every 16 ms
                last_update = perf_counter_ns()
                for (axm, id) in zip(axs, ids):
                    if id in datasets:
                        ds = datasets[id]
                        df = ds.to_dataframe()
                        if df.empty:
                            print(f"ID {id}> No data available")
                            continue
                        if id not in np.array(ncid[:]):
                            idx = len(ncid)
                            ncid[idx] = id
                            nctime[idx] = df['tstamp'].values # type: ignore
                            ncx[idx] = df['x'].values # type: ignore
                            ncy[idx] = df['y'].values # type: ignore
                            ncz[idx] = df['z'].values # type: ignore
                        else:
                            idx = np.where(ncid[:] == id)[0][0]
                            nctime[idx] = df['tstamp'].values # type: ignore
                            ncx[idx] = df['x'].values # type: ignore
                            ncy[idx] = df['y'].values # type: ignore
                            ncz[idx] = df['z'].values # type: ignore
                        # ncfile.variables['dz'][:, len(ids) - 1] = df['dz'].value
                        # else:
                        #     print(f"ID {id}> {len(df)} total points")
                        now = df['tstamp'].iloc[-1]
                        # Show last second of data
                        sel = df['tstamp'] > (now - 1e6)
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
                        # print(f"ID {id}> {len(tstamp)} points, time range {tstamp.iloc[0]} to {tstamp.iloc[-1]} ms")
                        # print(f'ID {id}> Y-axis limits: Y: ({ymin}, {ymax}), dY: ({dymin}, {dymax})')
                        for aid, ax in enumerate(axm):
                            ax.clear()
                            if aid == 0:
                                ax.plot(tstamp, df['x'][sel],
                                        label='X', color='red')
                                ax.plot(tstamp, df['y'][sel],
                                        label='Y', color='green')
                                ax.plot(tstamp, df['z'][sel],
                                        label='Z', color='blue')
                                # ax.set_ylim(ymin, ymax)
                                ax.set_ylim(-2, 2)
                            elif aid == 1:
                                ax.plot(tstamp, df['dx'][sel],
                                        label='dX', color='red')
                                ax.plot(tstamp, df['dy'][sel],
                                        label='dY', color='green')
                                ax.plot(tstamp, df['dz'][sel],
                                        label='dZ', color='blue')
                                ax.set_ylim(dymin, dymax)
                            ax.set_xlim(-1000, 0)

                fig.canvas.draw()
                fig.canvas.flush_events()

        except Exception as e:
            print(f"Error: {e}")
            client.close()
            break
        except KeyboardInterrupt:
            print("Interrupted by user")
            client.close()
            plt.close(fig)
            break

        if plt.fignum_exists(fignum) is False:
            print("Figure closed, exiting loop")
            client.close()
            break
    print("Done receiving data")

# %%
if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="WebSocket Client for Accelerometer Data")
    parser.add_argument('host', type=str, help='Host address of the WebSocket server', default='localhost', nargs='?')
    parser.add_argument('port', type=int, help='Port number of the WebSocket server', default=14389, nargs='?')
    args = parser.parse_args()
    run(args.host, args.port)