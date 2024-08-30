#ifndef MULTITOUCH_SIMULATOR_H
#define MULTITOUCH_SIMULATOR_H

#ifdef __cplusplus
extern "C" {
#endif

    typedef struct {
        int uinput_fd;
    } MultiTouchSimulator;

    MultiTouchSimulator* create_simulator();
    void destroy_simulator(MultiTouchSimulator* simulator);
    void touch_down(MultiTouchSimulator* simulator, int slot, int x, int y, int tracking_id);
    void touch_move(MultiTouchSimulator* simulator, int slot, int x, int y);
    void touch_up(MultiTouchSimulator* simulator, int slot);

#ifdef __cplusplus
}
#endif

#endif
