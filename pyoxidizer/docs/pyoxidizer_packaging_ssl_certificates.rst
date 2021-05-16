.. py:currentmodule:: starlark_pyoxidizer

.. _pyoxidizer_packaging_ssl_certificates:

=======================
SSL Certificate Loading
=======================

If using the :py:mod:`ssl` Python module (e.g. as part of making
connections to ``https://`` URLs), Python in its default configuration
will want to obtain a list of *trusted* X.509 / SSL certificates for
verifying connections.

If a list of trusted certificates cannot be found, you may encounter
errors like ``ssl.SSLCertVerificationError: [SSL:
CERTIFICATE_VERIFY_FAILED] certificate verify failed: unable to get
local issuer certificate``.

How Python Looks for Certificates
=================================

By default, Python will likely call :py:meth:`ssl.SSLContext.load_default_certs`
to load the *default certificates*.

On Windows, Python automatically loads certificates from the Windows
certificate store. This should *just work* with PyOxidizer.

On all platforms, Python attempts to load certificates from the default
locations compiled into the OpenSSL library that is being used. With
PyOxidizer, the OpenSSL (or LibreSSL) library is part of the Python
distribution used to produce a binary.

The OpenSSL library hard codes default certificate search paths.
For PyOxidizer's Python distributions, the paths are:

* (Windows) ``C:\Program Files\Common Files\SSL\cert.pem`` (file) and
  ``C:\Program Files\Common Files\SSL\certs`` (directory).
* (non-Windows) ``/etc/ssl/cert.pem`` (file) and ``/etc/ssl/certs``
  (directory).

In addition, OpenSSL (but not LibreSSL) will look for path overrides
in the ``SSL_CERT_FILE`` and ``SSL_CERT_DIR`` environment variables.

You can verify all of this behavior by calling
:py:func:`ssl.get_default_verify_paths`::

    $ python3.9
    Python 3.9.5 (default, Apr 16 2021, 08:56:35)
    [GCC 10.2.0] on linux
    Type "help", "copyright", "credits" or "license" for more information.
    >>> import ssl
    >>> ssl.get_default_verify_paths()
    DefaultVerifyPaths(cafile=None, capath='/etc/ssl/certs', openssl_cafile_env='SSL_CERT_FILE', openssl_cafile='/etc/ssl/cert.pem', openssl_capath_env='SSL_CERT_DIR', openssl_capath='/etc/ssl/certs')

On macOS, ``/etc/ssl`` *should* exist, as it is part of the standard macOS
install. So OpenSSL / Python should find certificates automatically.

On Windows, the default certificate path won't exist unless something
that isn't PyOxidizer materializes the aforementioned files/directories.
However, since Python loads certificates from the Windows certificate
store automatically, OpenSSL / Python should be able to load certificates
from PyOxidizer applications without issue.

On Linux, things are more complicated. The ``/etc/ssl`` directory is
common, but not ubiquitous. This directory likely exists on all Debian
based distributions, like Ubuntu. If the directory does not exist, OpenSSL /
Python will likely fail to find certificates and summarily fail to verify
connections against them.

Using Alternative Certificate Paths
===================================

PyOxidizer doesn't yet have a built-in mechanism for automatically
registering additional certificates or certificate paths at run-time.
Therefore, if OpenSSL / Python is unable to locate certificates, you
will need to add custom logic to your application to have it look for
additional certificates.

Certifi
-------

The `certifi <https://pypi.org/project/certifi/>`_ Python package provides
access to a copy of Mozilla's trusted certificates list. Using ``certifi``
enables you to have access to a known trusted certificates list without
dependence on certificates present in the run-time environment / operating
system.

Because ``certifi`` and its certificate list is distributed with your
application, it is guaranteed to be present and certificate loading should
*just work*.

To use ``certifi`` with PyOxidizer, you can install it as an additional
package. From your Starlark configuration file:

.. code-block:: python

    def make_exe():
        dist = default_python_distribution()
        exe = dist.to_python_executable(name="myapp")

        # Check for newer versions at https://pypi.org/project/certifi/.
        exe.add_python_resources(exe.pip_install(["certifi==2020.12.5"]))

        return exe

Then from your application's Python code:

.. code-block:: python

    import certifi
    import ssl

    # Obtain a default ssl.SSLContext but with certifi's certificate data loaded.
    ctx = ssl.create_default_context(cadata=certifi.contents())

    # Or if you already have an ssl.SSLContext instance and want to load
    # certifi's data in it:
    ctx.load_verify_locations(cadata=certifi.contents())

    # Various APIs that create connections also accept a `cadata` argument.
    # Under the hood they pass this argument to construct the ssl.SSLContext.
    # e.g. urllib.request.urlopen().
    import urllib.request
    urllib.request.urlopen(url, cadata=certifi.contents())

Manually Specifying Paths to Certificates
-----------------------------------------

If you know the paths to certificates to use, you can specify those
paths via various :py:mod:`ssl` APIs, often through the ``cafile`` and
``capath`` arguments. e.g.

.. code-block:: python

    import ssl

    ctx = ssl.create_default_context(capath="/path/to/ssl/certs")

    import urllib.request
    urllib.request.urlopen(url, capath="/path/to/ssl/certs")

Using Environment Variables
---------------------------

OpenSSL (but not LibreSSL) will look for the ``SSL_CERT_FILE`` and
``SSL_CERT_DIR`` environment variables to automatically set the CA
file and directory, respectively.

You can set these within your process to point to alternative paths. e.g.

.. code-block:: python

    import os

    os.environ["SSL_CERT_DIR"] = "/path/to/ssl/certs"
