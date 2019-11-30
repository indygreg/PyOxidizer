import appdirs
import _cffi_backend

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
