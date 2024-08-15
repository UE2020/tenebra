#ifndef MULTITOUCH_SIMULATOR_H
#define MULTITOUCH_SIMULATOR_H

typedef struct {
    int uinput_fd;
} MultiTouchSimulator;

MultiTouchSimulator* create_simulator(int width, int height);
void destroy_simulator(MultiTouchSimulator *simulator);
void touch_down(MultiTouchSimulator *simulator, int slot, int x, int y, int tracking_id);
void touch_move(MultiTouchSimulator *simulator, int slot, int x, int y);
void touch_up(MultiTouchSimulator *simulator, int slot);

#endif
