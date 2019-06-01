# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file is concatenated to importlib/_bootstrap.py. It essentially
# defines and registers our in-memory module importer.


class MemoryFinder:
    """Implements importlib.abc.MetaPathFinder to find modules in memory."""

    def __init__(self, modules):
        self._modules = modules

    # Start of importlib.abc.MetaPathFinder interface.

    def find_spec(self, fullname, path, target=None):
        if not self._modules.has_module(fullname):
            return None

        is_package = self._modules.is_package(fullname)

        # TODO consider setting origin and has_location so __file__ will
        # be populated.
        return _bootstrap.ModuleSpec(fullname, self,
                                     is_package=is_package)

    def find_module(self, fullname, path):
        raise NotImplementedError

    def invalidate_caches(self):
        pass

    # End of importlib.abc.MetaPathFinder interface.

    # Start of importlib.abc.Loader interface.

    def create_module(self, spec):
        pass

    def exec_module(self, module):
        name = module.__name__

        try:
            code = self._modules.get_code(name)
        except KeyError:
            raise ImportError('cannot find code in memory', name=name)

        code = marshal.loads(code)

        _bootstrap._call_with_frames_removed(exec, code, module.__dict__)

    # End of importlib.abc.Loader interface.

    # Start of importlib.abc.InspectLoader interface.

    def get_code(self, fullname):
        try:
            code = self._modules.get_code(fullname)
        except KeyError:
            raise ImportError('cannot find code in memory', name=fullname)

        return marshal.loads(code)

    def get_source(self, fullname):
        try:
            source = self._modules.get_source(fullname)
        except KeyError:
            raise ImportError('source not available', name=fullname)

        return decode_bytes(source)

    # End of importlib.abc.InspectLoader interface.

# This overrides importlib._bootstrap_external:_install()

def _install(_bootstrap_module):
    """Install the path-based import components."""
    _setup(_bootstrap_module)

    import _pymodules
    _pymodules._setup(_pymodules, _bootstrap_module)

    memory_finder = MemoryFinder(_pymodules.MODULES)

    sys.meta_path.append(memory_finder)

    #supported_loaders = _get_supported_file_loaders()
    #sys.path_hooks.extend([FileFinder.path_hook(*supported_loaders)])
    #sys.meta_path.append(PathFinder)
