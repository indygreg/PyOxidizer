-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

Format: 3.0 (quilt)
Source: libzstd
Binary: libzstd-dev, libzstd1, zstd, libzstd1-udeb
Architecture: any
Version: 1.4.8+dfsg-3
Maintainer: Debian Med Packaging Team <debian-med-packaging@lists.alioth.debian.org>
Uploaders: Kevin Murray <kdmfoss@gmail.com>, Olivier Sallou <osallou@debian.org>, Alexandre Mestiashvili <mestia@debian.org>
Homepage: https://github.com/facebook/zstd
Standards-Version: 4.6.0
Vcs-Browser: https://salsa.debian.org/med-team/libzstd
Vcs-Git: https://salsa.debian.org/med-team/libzstd.git
Testsuite: autopkgtest
Build-Depends: d-shlibs, debhelper-compat (= 13), help2man, liblz4-dev, liblzma-dev, zlib1g-dev
Package-List:
 libzstd-dev deb libdevel optional arch=any
 libzstd1 deb libs optional arch=any
 libzstd1-udeb udeb debian-installer optional arch=any
 zstd deb utils optional arch=any
Checksums-Sha1:
 a24e4ccf9fc356aeaaa0783316a26bd65817c354 1331996 libzstd_1.4.8+dfsg.orig.tar.xz
 896a47a2934d0fcf9faa8397d05a12b932697d1f 12184 libzstd_1.4.8+dfsg-3.debian.tar.xz
Checksums-Sha256:
 1e8ce5c4880a6d5bd8d3186e4186607dd19b64fc98a3877fc13aeefd566d67c5 1331996 libzstd_1.4.8+dfsg.orig.tar.xz
 fecd87a469d5a07b6deeeef53ed24b2f1a74ee097ce11528fe3b58540f05c147 12184 libzstd_1.4.8+dfsg-3.debian.tar.xz
Files:
 943bed8b8d98a50c8d8a101b12693bb4 1331996 libzstd_1.4.8+dfsg.orig.tar.xz
 4d2692830e1f481ce769e2dd24cbc9db 12184 libzstd_1.4.8+dfsg-3.debian.tar.xz

-----BEGIN PGP SIGNATURE-----

iQJFBAEBCgAvFiEE8fAHMgoDVUHwpmPKV4oElNHGRtEFAmFdVM4RHHRpbGxlQGRl
Ymlhbi5vcmcACgkQV4oElNHGRtF0Kg//bLydBw+YSjs1UYLmuL3jWvQqwhEyLwlK
Inao4bAJ13CjPXuALuCOz0XVyF/J480PiLAjqoBlIsm0zBITdaWik0P4ZWh//hnT
oQQEXI3dlHykHVYJ9ZXz9zAHWWjrs8tmTMGj2xQ479fTu1XTGQaWHuej5+TBhvIL
9S0kjtzGNnjsMPNdydToCkFY51KEBwvcaeVa1wFgrhHgu6SytPCJ27czm/HkkAvu
XFWnikKSMfzeqA+lctwxxRwlEDdCO3JW9jES8418Ucw+I9Lx4FMIJhOLiq12vxzd
LVAN7sWXRm6UjO1deZSuXEG9RBgUGtyzx0HIT01l38dpJ/z23dvl+E6xtoSXumIw
KMMBddplNjcOd+IS011e6jlX9Ht2XdLJQ92Z+FAIvV+DiAX0SVQZuJkxSZ6PsJ+3
03cEL0FbSLCW9XHcQe3YoyF/rAQhSnrNKkYdkGRYpo13qpWDojY1EVjLoIe2gYv2
asNXQCTmp5TrfkgpkHeB3yXo/CzzecB9Ewfpam0iB7mYTS0siNasK5gboOSz0urp
I9V2pG/UDClKXbmNIC8etScG2ZwH70jQddFcCnReUkGNOdubdPRBlEGahxPgh1aD
rSdD+NE9tV9EqJMAb381iKMBLVsmKB3eonHBMKYIz4VKTeq31vQs2O4ig5en4ktE
FRIa3Zc3eUU=
=ZzNx
-----END PGP SIGNATURE-----
