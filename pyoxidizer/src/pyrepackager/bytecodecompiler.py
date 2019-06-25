# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This script is executed to compile Python bytecode by the repackager.
#
# When invoked, we start a server that listens for commands. We then
# react to those commands and send results to the caller.

import marshal
import os
import re
import sys


RE_CODING = re.compile(b'^[ \t\f]*#.*?coding[:=][ \t]*([-_.a-zA-Z0-9]+)')


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

        # Default source encoding is UTF-8. But per PEP 263, the first or second
        # line of source can match a regular expression to define a custom
        # encoding. We need to detect custom encodings and use it to decode
        # the passed bytes to str.
        encoding = 'utf-8'

        for line in source.splitlines()[0:2]:
            m = RE_CODING.match(line)
            if m:
                encoding = m.group(1).decode('ascii')
                break

        # Someone has set us up the BOM! According to PEP 263 the file should
        # be interpreted as UTF-8.
        if source.startswith(b'\xef\xbb\xbf'):
            encoding = 'utf-8'
            source = source[3:]

        source = source.decode(encoding)

        code = compile(source, name, 'exec', optimize=optimize_level)
        bytecode = marshal.dumps(code)

        stdout.write(b'%d\n' % len(bytecode))
        stdout.write(bytecode)
        stdout.flush()
    else:
        raise Exception('invalid command: %s' % command)
