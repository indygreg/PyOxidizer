.. _contributing:

==========================
Contributing to PyOxidizer
==========================

This page documents how to contribute to PyOxidizer.

As a User
=========

PyOxidizer is currently a relative young project and could substantially
benefit from reports from its users.

Try to package applications with PyOxidizer. If things break or are
hard to learn, `file an issue <https://github.com/indygreg/PyOxidizer/issues>`_
on GitHub.

You can also join the
`pyoxidizer-users <https://groups.google.com/forum/#!forum/pyoxidizer-users>`_
mailing list to report your experience, get in touch with other
users, etc.

As a Developer
==============

If you would like to contribute to the code behind PyOxidizer, you can
do so using a standard GitHub workflow through the canonical project
home at https://github.com/indygreg/PyOxidizer.

Please note that PyOxidizer's maintainer can be quite busy from time to
time. So please be patient. He will be patient with you.

The documentation around how to hack on the PyOxidizer codebase is a bit
lacking. Sorry for that!

The most important command for contributors to know how to run is
``cargo run --bin pyoxidizer``. This will compile the ``pyoxidizer`` executable
program and run it. Use it like ``cargo run --bin pyoxidizer -- init
~/tmp/myapp`` to run ``pyoxidizer init ~/tmp/myapp`` for example. If you
just run ``cargo build``, it will also build the ``pyapp`` project, which
is an in-repo project that attempts to use PyOxidizer.

Financial Contributions
=======================

If you would like to thank the PyOxidizer maintainer via a financial
contribution, you can do so
`on his Patreon <https://www.patreon.com/indygreg>`_ or
`via PayPal <https://www.paypal.com/cgi-bin/webscr?cmd=_donations&business=gregory%2eszorc%40gmail%2ecom&lc=US&item_name=PyOxidizer&currency_code=USD&bn=PP%2dDonationsBF%3abtn_donate_LG%2egif%3aNonHosted>`_.

Financial contributions of any amount are appreciated. Please do not
feel obligated to donate money: only donate if you are financially
able and feel the maintainer deserves the reward for a job well done.
