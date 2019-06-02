# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file is concatenated to importlib/_bootstrap.py.

# This overrides importlib._bootstrap_external:_install()
def _install(_bootstrap_module):
    # We need to call the original setup function.
    _setup(_bootstrap_module)

    # These lines magically register the PyOxidizer importer.
    import _pyoxidizer_importer
    _pyoxidizer_importer._setup(_pyoxidizer_importer, _bootstrap_module, marshal, decode_source)

    #supported_loaders = _get_supported_file_loaders()
    #sys.path_hooks.extend([FileFinder.path_hook(*supported_loaders)])
    #sys.meta_path.append(PathFinder)
