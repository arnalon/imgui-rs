#!/bin/bash
set -euo pipefail

# this should be a path on your local, like `~/Documents/imgui` to the Dear ImGui repository.
IMGUI_DIR=${1:?}

SCRIPT_DIR=$(dirname ${0} | python3 -c 'import os, sys; print(os.path.abspath(sys.stdin.read().strip()))' )

cd ${SCRIPT_DIR}
./_update-imgui.sh $IMGUI_DIR v1.92.7 ./imgui-master/imgui
# arcdps fork: docking variants are unused (arcdps builds the release branch).
#./_update-imgui.sh $IMGUI_DIR v1.91.3-docking ./imgui-docking/imgui

./_update-imgui.sh $IMGUI_DIR v1.92.7 ./imgui-master-freetype/imgui
#./_update-imgui.sh $IMGUI_DIR v1.91.3-docking ./imgui-docking-freetype/imgui
