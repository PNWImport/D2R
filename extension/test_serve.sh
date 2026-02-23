#!/bin/bash
# Serves the test harness on http://localhost:8080
# Usage: cd extension && bash test_serve.sh
PORT=${1:-8080}
echo "Serving test harness at http://localhost:$PORT/test_gui.html"
echo "Press Ctrl+C to stop."
python3 -m http.server "$PORT"
