// MIT: VectorDash adapter. LibRaw is separately distributed under CDDL-1.0.
// One read-only file, bounded output, no files written and no persistent process.
#include <libraw/libraw.h>
#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <fcntl.h>
#include <io.h>
#include <memory>
#include <vector>

static constexpr uint64_t PIXELS = 16000000;
static constexpr uint64_t OUTPUT = 48 * 1024 * 1024;
static int fail(const char* message) { fprintf(stderr, "%s", message); return 1; }
static bool emit(libraw_processed_image_t* image, unsigned flip) {
    if (!image || image->data_size == 0 || image->data_size > OUTPUT) return false;
    if (image->type != LIBRAW_IMAGE_JPEG &&
        (image->type != LIBRAW_IMAGE_BITMAP || image->bits != 8 || image->colors != 3 ||
         uint64_t(image->width) * image->height > PIXELS ||
         uint64_t(image->width) * image->height * 3 != image->data_size)) return false;
    uint32_t header[] = {0x31524456, unsigned(image->type), image->width, image->height, flip, image->data_size};
    return fwrite(header, sizeof(header), 1, stdout) == 1 &&
           fwrite(image->data, 1, image->data_size, stdout) == image->data_size;
}
using Memory = std::unique_ptr<libraw_processed_image_t, decltype(&LibRaw::dcraw_clear_mem)>;
int wmain(int argc, wchar_t** argv) {
    _setmode(_fileno(stdout), _O_BINARY);
    // Parent assigns the memory-limited Windows Job before releasing this gate.
    if (argc != 3 || getchar() != 'G') return fail("Invalid RAW worker request");
    auto owner = std::make_unique<LibRaw>(LIBRAW_OPTIONS_NO_DATAERR_CALLBACK);
    auto &raw = *owner;
    raw.imgdata.rawparams.max_raw_memory_mb = 128;
    int code = raw.open_file(argv[1]);
    if (code != LIBRAW_SUCCESS) return fail(LibRaw::strerror(code));
    unsigned flip = raw.imgdata.sizes.flip < 0 ? 0 : raw.imgdata.sizes.flip;
    if (wcscmp(argv[2], L"preview") == 0) {
        auto list = raw.imgdata.thumbs_list;
        std::vector<int> candidates;
        for (int i = 0; i < std::min(list.thumbcount, int(LIBRAW_THUMBNAIL_MAXCOUNT)); ++i) {
            auto &t = list.thumblist[i];
            uint64_t pixels = uint64_t(t.twidth) * t.theight;
            if (t.tlength > 0 && t.tlength <= OUTPUT && pixels <= PIXELS) candidates.push_back(i);
        }
        // Smallest camera preview >= 1MP; otherwise largest available thumbnail.
        std::sort(candidates.begin(), candidates.end(), [&](int a, int b) {
            uint64_t x = uint64_t(list.thumblist[a].twidth) * list.thumblist[a].theight;
            uint64_t y = uint64_t(list.thumblist[b].twidth) * list.thumblist[b].theight;
            if ((x >= 1000000) != (y >= 1000000)) return x >= 1000000;
            return x >= 1000000 ? x < y : x > y;
        });
        if (candidates.empty()) candidates.push_back(-1);
        raw.recycle();
        for (int i : candidates) {
            auto candidate = std::make_unique<LibRaw>(LIBRAW_OPTIONS_NO_DATAERR_CALLBACK);
            auto &preview = *candidate;
            preview.imgdata.rawparams.max_raw_memory_mb = 128;
            if (preview.open_file(argv[1]) != LIBRAW_SUCCESS) continue;
            code = i < 0 ? preview.unpack_thumb() : preview.unpack_thumb_ex(i);
            if (code != LIBRAW_SUCCESS) continue;
            int error = 0;
            Memory image(preview.dcraw_make_mem_thumb(&error), &LibRaw::dcraw_clear_mem);
            unsigned rotation = i >= 0 && list.thumblist[i].tflip <= 7 ? list.thumblist[i].tflip : flip;
            if (image && emit(image.get(), rotation)) return 0;
        }
        return fail("No usable embedded JPEG/RGB preview; enable RAW development fallback to try decoding the sensor data");
    }
    if (wcscmp(argv[2], L"develop") != 0) return fail("Unknown RAW operation");
    if (uint64_t(raw.imgdata.sizes.raw_width) * raw.imgdata.sizes.raw_height > 50000000)
        return fail("RAW sensor exceeds the 50MP development budget; embedded previews can still be used");
    raw.imgdata.params.half_size = 1;
    raw.imgdata.params.user_flip = 0; // Rust applies orientation exactly once.
    raw.imgdata.params.output_bps = 8;
    raw.imgdata.params.output_color = 1;
    raw.imgdata.params.use_camera_wb = 1;
    raw.imgdata.params.no_auto_bright = 1;
    if ((code = raw.unpack()) != LIBRAW_SUCCESS || (code = raw.dcraw_process()) != LIBRAW_SUCCESS)
        return fail(LibRaw::strerror(code));
    int error = 0;
    Memory image(raw.dcraw_make_mem_image(&error), &LibRaw::dcraw_clear_mem);
    if (!image || !emit(image.get(), flip)) return fail("RAW output exceeded the image budget or was unsupported");
    return 0;
}
