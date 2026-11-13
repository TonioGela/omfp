#!/bin/sh
# ENV-065: omfp in its Flatpak. The daemon's data and the config file live where every other omfp install has them
# (~/.local/share/omfp, ~/.config/omfp), not in ~/.var/app: the host's `omfp` commands and a daemon started outside
# the sandbox share them (DMN-007, DMN-009).
export XDG_DATA_HOME="${HOST_XDG_DATA_HOME:-$HOME/.local/share}"
export XDG_CONFIG_HOME="${HOST_XDG_CONFIG_HOME:-$HOME/.config}"
exec /app/lib/omfp/omfp "$@"
