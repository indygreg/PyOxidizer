import appdirs
import _cffi_backend
# Allow AttributeError rather than ImportError
# as this indirectly triggers a missing __file__
# https://github.com/dabeaz/ply/issues/216
try:
    import zero_buffer
except AttributeError:
    pass

print("hello, world")
