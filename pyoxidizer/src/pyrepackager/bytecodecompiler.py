# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This script is executed to compile Python bytecode by the repackager.
#
# When invoked, we start a server that listens for commands. We then
# react to those commands and send results to the caller.

import marshal
import os
import sys


if marshal.version != 4:
    raise Exception('unexpected marshal version: %d' % marshal.version)

stdin = sys.__stdin__.buffer
stdout = sys.__stdout__.buffer


while True:
    command = stdin.readline().rstrip()

    if command == b'exit':
        sys.exit(0)
    elif command == b'compile':
        name_len = stdin.readline().rstrip()
        source_len = stdin.readline().rstrip()
        optimize_level = stdin.readline().rstrip()

        name_len = int(name_len)
        source_len = int(source_len)
        optimize_level = int(optimize_level)

        name = stdin.read(name_len)
        source = stdin.read(source_len)

        name = os.fsdecode(name)
        source = source.decode('latin1')

        code = compile(source, name, 'exec', optimize=optimize_level)
        bytecode = marshal.dumps(code)

        stdout.write(b'%d\n' % len(bytecode))
        stdout.write(bytecode)
        stdout.flush()
    else:
        raise Exception('invalid command: %s' % command)
