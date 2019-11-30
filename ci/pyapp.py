import appdirs
import _cffi_backend

import markupsafe._speedups
import simplejson._speedups

import markupsafe
import simplejson

print("hello, world")

import cryptography.fernet

m = b'does this work?'
s = 'original message:\n' + str(m) + '\n\n'

k = cryptography.fernet.Fernet.generate_key()
fernet = cryptography.fernet.Fernet(k)
t = fernet.encrypt(m)
d = fernet.decrypt(t)

s += 'key:\n' + str(k) + '\n\n'
s += 'token:\n' + str(t) + '\n\n'
s += 'decoded message:\n' + str(d)
print(s)
assert d == m

# This only confirms the built objects are loadable.
import gevent
import gevent._queue
assert 'gevent' in dir(gevent._queue)

import gevent._event
import gevent._greenlet
import gevent._local
import gevent.resolver.cares
import gevent.libev.corecext
