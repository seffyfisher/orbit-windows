#!/usr/bin/env python3
import os, shutil, sys
python=os.environ.get('ORBIT_ZIG_PYTHON')
command=[python,'-m','ziglang'] if python else [os.environ.get('ORBIT_ZIG',shutil.which('zig') or 'zig')]
os.execvp(command[0],[*command,'dlltool',*sys.argv[1:]])
