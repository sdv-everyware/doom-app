# This program is free software: you can redistribute it and/or modify it under the terms of
# the GNU General Public License as published by the Free Software Foundation, either version 3
# of the License, or (at your option) any later version.
#
# This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
# without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See
# the GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License along with this program.
# If not, see <https://www.gnu.org/licenses/>.

# The game of the panel (doom-app, README.md) in a product: a board product inherits this file.
# It is experimental and off by default. Turn it on with EVERYWARE_GAME=true (in the environment
# or the product): the image then gets doom_app, Freedoom's WAD read-only at
# /product/etc/doom/freedoom2.wad (the module scripts/fetch-freedoom.sh writes into wad/, so the
# WAD must be fetched before the build) and ro.vendor.everyware.game=true, by which the panel
# shows its game button. Nothing starts doom_app in the image: a service and a SELinux domain for
# it are not made (README.md); it is started from adb.
EVERYWARE_GAME ?= false
ifeq ($(EVERYWARE_GAME),true)
ifeq ($(wildcard packages/services/display_safety/utils/doom-app/wad/Android.bp),)
$(error EVERYWARE_GAME=true needs Freedoom's WAD: run packages/services/display_safety/utils/doom-app/scripts/fetch-freedoom.sh first)
endif
PRODUCT_PACKAGES += \
    doom_app \
    doom_app_freedoom2_wad
PRODUCT_VENDOR_PROPERTIES += ro.vendor.everyware.game=true
endif
