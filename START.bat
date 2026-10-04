@echo off
cd /d "%~dp0"
python -m pip install -q -r requirements.txt
start "" pythonw "%~dp0portal_keeper.py"
exit
