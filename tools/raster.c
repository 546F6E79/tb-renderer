/* Крошечный шим над freetype для tools/gen_atlas.py:
 * вся работа со структурами — здесь, компилятор знает смещения. */
#include <string.h>
#include <ft2build.h>
#include FT_FREETYPE_H

static FT_Library lib;
static FT_Face face;

int tb_open(const char *path, int px) {
    if (!lib && FT_Init_FreeType(&lib)) return 1;
    if (face) { FT_Done_Face(face); face = 0; }
    if (FT_New_Face(lib, path, 0, &face)) return 2;
    FT_Set_Pixel_Sizes(face, 0, (FT_UInt)px);
    return 0;
}

int tb_render(unsigned long cp, unsigned *w, unsigned *h,
              int *left, int *top, int *adv,
              unsigned char *buf, int buflen) {
    if (!face) return 3;
    if (FT_Load_Char(face, (FT_ULong)cp, FT_LOAD_RENDER)) return 1;
    FT_GlyphSlot g = face->glyph;
    FT_Bitmap *b = &g->bitmap;
    if ((int)(b->rows * b->width) > buflen) return 2;
    for (unsigned r = 0; r < b->rows; r++)
        memcpy(buf + (size_t)r * b->width, b->buffer + (size_t)r * b->pitch, b->width);
    *w = b->width; *h = b->rows;
    *left = g->bitmap_left; *top = g->bitmap_top;
    *adv = (int)(g->advance.x >> 6);
    return 0;
}
