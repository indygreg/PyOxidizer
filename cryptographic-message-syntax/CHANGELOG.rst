========================================
``cryptographic-message-syntax`` History
========================================

0.17.0
======

(Not yet released)

* bcder crate upgraded from 0.6.1 to 0.7.0. This entailed a lot of changes,
  mainly to error handling.
* ``SignedAttributes`` should now be sorted properly. Previous versions had a
  sorting mechanism that was only partially correct and would result in
  incorrect sorting for some inputs. The old behavior could have resulted in
  incorrect signatures being produced or validations incorrectly failing. (#614)