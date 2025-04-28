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
    void touch_down(MultiTouchSimulator* simulator, int slot, int x, int y, int tracking_id);
    void touch_move(MultiTouchSimulator* simulator, int slot, int x, int y);
    void touch_up(MultiTouchSimulator* simulator, int slot);
    void move_mouse_relative(MultiTouchSimulator* simulator, int x, int y);
    void scroll_vertically(MultiTouchSimulator* simulator, int value);
    void scroll_horizontally(MultiTouchSimulator* simulator, int value);
    void pen(MultiTouchSimulator* simulator, int x, int y, double pressure, int tilt_x, int tilt_y);

#ifdef __cplusplus
}
#endif

#endif
