# %%
import matplotlib.pyplot as plt
from netCDF4 import Dataset
import numpy as np
# %%
def plot_nc(filename):
    ncfile = Dataset(filename, 'r')
    glen = len(ncfile.groups)
    fig, axs = plt.subplots(glen, 1, dpi = 150)
    for gname, ax in zip(ncfile.groups.keys(), axs):
        print(f"Plotting group: {gname}")
        group = ncfile.groups[gname]
        tstamp = group.variables['tstamp'][:]
        arg = np.argsort(tstamp)
        tstamp = tstamp[arg]
        x = group.variables['x'][:]
        x = x[arg]
        y = group.variables['y'][:]
        y = y[arg]
        z = group.variables['z'][:]
        z = z[arg]

        ax.plot(tstamp, x, label='X', color='blue')
        ax.plot(tstamp, y, label='Y', color='green')
        ax.plot(tstamp, z, label='Z', color='red')

        ax.set_title(gname)
        ax.set_xlabel('Time')
        ax.set_ylabel('Acceleration (g)')
        ax.legend()
    
    fig.tight_layout()
    fig.show()

# %%
if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description='Plot NetCDF data')
    parser.add_argument('filename', type=str, help='Path to the NetCDF file')
    args = parser.parse_args()
    plot_nc(args.filename)
