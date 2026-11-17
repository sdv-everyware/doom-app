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
# It is experimental and off by default. Turn it on with EVERYWARE_GAME=true (in the environment or
# the product): the image then gets doom_app on the vendor partition, Freedoom's WAD read-only at
# /vendor/etc/doom/freedoom2.wad (the module scripts/fetch-freedoom.sh writes into wad/, so the
# WAD must be fetched before the build), the game's service in /vendor/etc/init/everyware/ (a
# directory the board's root init imports; started when the panel runs, README.md "How it
# starts") and ro.vendor.everyware.game=true, by which the panel shows its game button. The
# board's policy gives doom_app its domain everyware_game.
EVERYWARE_GAME ?= false
ifeq ($(EVERYWARE_GAME),true)
ifeq ($(wildcard packages/services/display_safety/utils/doom-app/wad/Android.bp),)
$(error EVERYWARE_GAME=true needs Freedoom's WAD: run packages/services/display_safety/utils/doom-app/scripts/fetch-freedoom.sh first)
endif
PRODUCT_PACKAGES += \
    doom_app \
    doom_app_freedoom2_wad
PRODUCT_COPY_FILES += \
    packages/services/display_safety/utils/doom-app/product/everyware_game.rc:$(TARGET_COPY_OUT_VENDOR)/etc/init/everyware/everyware_game.rc
PRODUCT_VENDOR_PROPERTIES += ro.vendor.everyware.game=true
endif
