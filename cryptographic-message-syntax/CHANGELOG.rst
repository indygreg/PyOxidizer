========================================
``cryptographic-message-syntax`` History
========================================

0.18.0
======

(Not yet released)

0.17.0
======

(Released 2022-0807)

* bcder crate upgraded from 0.6.1 to 0.7.0. This entailed a lot of changes,
  mainly to error handling.
* ``SignedAttributes`` should now be sorted properly. Previous versions had a
  sorting mechanism that was only partially correct and would result in
  incorrect sorting for some inputs. The old behavior could have resulted in
  incorrect signatures being produced or validations incorrectly failing. (#614)
* The crate now re-exports some symbols for 3rd party crates ``bcder::Oid`` and
  ``bytes::Bytes``.
* Support for creating *external signatures*, which are signatures over external
  content not stored inline in produced signatures. (#614)
* (API change) ``SignedDataBuilder::signed_content()`` has effectively been
  renamed to ``content_inline()``. (#614)
