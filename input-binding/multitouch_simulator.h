#ifndef _MULTITOUCH_SIMULATOR_H
#define _MULTITOUCH_SIMULATOR_H

#ifdef __cplusplus
extern "C" {
#endif

    typedef struct {
        int touch_fd;
    } MultiTouchSimulator;

    MultiTouchSimulator* create_simulator();
    void destroy_simulator(MultiTouchSimulator* simulator);
    void scroll_vertically(MultiTouchSimulator* simulator, int value);
    void scroll_horizontally(MultiTouchSimulator* simulator, int value);
    void touch_down(MultiTouchSimulator* simulator, int slot, int x, int y, int tracking_id);
    void touch_move(MultiTouchSimulator* simulator, int slot, int x, int y);
    void touch_up(MultiTouchSimulator* simulator, int slot);

#ifdef __cplusplus
}
#endif

#endif
