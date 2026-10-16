#!/usr/bin/env python3
"""Optional macOS/Linux cross-linker. Installed users need no build tools."""
import os, shutil, sys
drop={'-Wl,--disable-auto-image-base','-Wl,--high-entropy-va','-lgcc_eh','-lgcc','-l:libpthread.a','-lmsvcrt'}
args=[a for a in sys.argv[1:] if a not in drop and 'libcompiler_builtins-' not in a]
python=os.environ.get('ORBIT_ZIG_PYTHON')
command=[python,'-m','ziglang'] if python else [os.environ.get('ORBIT_ZIG',shutil.which('zig') or 'zig')]
os.execvp(command[0],[*command,'cc','-target','x86_64-windows-gnu','-lc',*args])
