.. _apple_codesign_debugging:

================================
How to Debug and Report Problems
================================

Apple code signing is complex and there will be cases where this tool
behaves differently from Apple's, possibly to the point where Apple rejects
the output of this tool.

.. important::

   If Apple software rejects the output of this tool, we consider that a bug.
   We encourage end-users to report these bugs to the
   `GitHub issue tracker <https://github.com/indygreg/PyOxidizer/issues>`_.

Commands to Print Signature Info
================================

The ``rcodesign print-signature-info`` command can be used to dump YAML
describing any signable file entity. Just point it at a Mach-O, bundle, DMG,
or ``.pkg`` installer and it will tell you what it knows about the entity.

The ``rcodesign diff-signatures`` command will internally execute
``print-signature-info`` against 2 paths and print the differences between them.

``rcodesign diff-signatures`` is exceptionally useful at understanding
differences in behavior between this tool and Apple's. If Apple is rejecting
the output of this tool, comparing the output of the same operation with Apple's
tooling against this tool's is a good way to find the source of the problem.

Reporting Actionable Bugs
=========================

Please include the following in bug reports to improve chances for action:

* The released version or Git commit that this tool was built from.
* The command line used.
* The full output of the command.
* The output of ``rcodesign diff-signatures`` comparing similar operations
  between Apple's tooling and ours.
* A copy of the entity you were attempting to sign.
* Text copy or screenshot of error from Apple tooling indicating what failed.

It is understandable that some people may not desire to file publish issue
reports or submit a copy of their application to be seen by the world. If
you send a polite email to gregory.szorc@gmail.com with ``apple-codesign`` or
``rcodesign`` in the subject line along with more private/sensitive details,
support can be given over email.
