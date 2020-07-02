# ilbm
Image decoded/encoder for Amiga ILBM/LBM files

This is mainly a library, not a viewer, to load various types of Amiga style ILBM files (and maybe even to write some of them).
My hope is this code can eventually be linked into the image crate.

There is an example using SDL2, that will display loaded images, it is pretty clunky and tricky to use (due to SDL and the way it
is linked) so it is mainly illustrative, and useful for testing

I know that ILBM is largely of interest only as a historical format, and less and less programs support it, those that do
are spotty and support some variations and not others. I hope with a current and well tested decoder, old Amiga assets can live again

Thanks to those in the Amiga community who provided some of the sample images used in testing.

