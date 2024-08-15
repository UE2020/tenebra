#include <errno.h>
#include <fcntl.h>
#include <linux/uinput.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <stdlib.h>

// Define a struct to hold the state for the multitouch simulator
typedef struct {
    int uinput_fd;
} MultiTouchSimulator;

// Function to setup the device
void setup_device(MultiTouchSimulator *simulator, int width, int height) {
    struct uinput_user_dev uidev;
    memset(&uidev, 0, sizeof(uidev));

    strncpy(uidev.name, "Multi-Touch Device", UINPUT_MAX_NAME_SIZE);
    uidev.id.bustype = BUS_USB;
    uidev.id.vendor = 0x1234;
    uidev.id.product = 0x5678;
    uidev.id.version = 1;

    ioctl(simulator->uinput_fd, UI_SET_EVBIT, EV_ABS);
    ioctl(simulator->uinput_fd, UI_SET_ABSBIT, ABS_MT_SLOT);
    ioctl(simulator->uinput_fd, UI_SET_ABSBIT, ABS_MT_POSITION_X);
    ioctl(simulator->uinput_fd, UI_SET_ABSBIT, ABS_MT_POSITION_Y);
    ioctl(simulator->uinput_fd, UI_SET_ABSBIT, ABS_MT_TRACKING_ID);

    uidev.absmin[ABS_MT_POSITION_X] = 0;
    uidev.absmax[ABS_MT_POSITION_X] = width;
    uidev.absmin[ABS_MT_POSITION_Y] = 0;
    uidev.absmax[ABS_MT_POSITION_Y] = height;

    uidev.absmin[ABS_MT_SLOT] = 0;
    uidev.absmax[ABS_MT_SLOT] = 9;

    uidev.absmin[ABS_MT_TRACKING_ID] = 0;
    uidev.absmax[ABS_MT_TRACKING_ID] = 65535;

    if (write(simulator->uinput_fd, &uidev, sizeof(uidev)) < 0) {
        fprintf(stderr, "Error setting up device: %s\n", strerror(errno));
        exit(1);
    }

    ioctl(simulator->uinput_fd, UI_SET_EVBIT, EV_SYN);
    ioctl(simulator->uinput_fd, UI_SET_PROPBIT, INPUT_PROP_DIRECT);

    if (ioctl(simulator->uinput_fd, UI_DEV_CREATE) < 0) {
        fprintf(stderr, "Error creating uinput device: %s\n", strerror(errno));
        exit(1);
    }
}

// Function to emit input events
void emit_event(int uinput_fd, int type, int code, int value) {
    struct input_event ev;
    memset(&ev, 0, sizeof(ev));
    ev.type = type;
    ev.code = code;
    ev.value = value;
    ev.time.tv_sec = 0;
    ev.time.tv_usec = 0;

    if (write(uinput_fd, &ev, sizeof(ev)) < 0) {
        fprintf(stderr, "Error writing event: %s\n", strerror(errno));
        exit(1);
    }
}

// Function to initialize the simulator
MultiTouchSimulator* create_simulator(int width, int height) {
    MultiTouchSimulator *simulator = (MultiTouchSimulator*) malloc(sizeof(MultiTouchSimulator));
    simulator->uinput_fd = open("/dev/uinput", O_WRONLY | O_NONBLOCK);
    if (simulator->uinput_fd < 0) {
        fprintf(stderr, "Error opening /dev/uinput: %s\n", strerror(errno));
        exit(1);
    }
    setup_device(simulator, width, height);
    return simulator;
}

// Function to destroy the simulator
void destroy_simulator(MultiTouchSimulator *simulator) {
    ioctl(simulator->uinput_fd, UI_DEV_DESTROY);
    close(simulator->uinput_fd);
    free(simulator);
}

// Function to simulate touch down
void touch_down(MultiTouchSimulator *simulator, int slot, int x, int y, int tracking_id) {
    emit_event(simulator->uinput_fd, EV_ABS, ABS_MT_SLOT, slot);
    emit_event(simulator->uinput_fd, EV_ABS, ABS_MT_TRACKING_ID, tracking_id);
    emit_event(simulator->uinput_fd, EV_ABS, ABS_MT_POSITION_X, x);
    emit_event(simulator->uinput_fd, EV_ABS, ABS_MT_POSITION_Y, y);
    emit_event(simulator->uinput_fd, EV_SYN, SYN_REPORT, 0);
}

// Function to simulate touch move
void touch_move(MultiTouchSimulator *simulator, int slot, int x, int y) {
    emit_event(simulator->uinput_fd, EV_ABS, ABS_MT_SLOT, slot);
    emit_event(simulator->uinput_fd, EV_ABS, ABS_MT_POSITION_X, x);
    emit_event(simulator->uinput_fd, EV_ABS, ABS_MT_POSITION_Y, y);
    emit_event(simulator->uinput_fd, EV_SYN, SYN_REPORT, 0);
}

// Function to simulate touch up
void touch_up(MultiTouchSimulator *simulator, int slot) {
    emit_event(simulator->uinput_fd, EV_ABS, ABS_MT_SLOT, slot);
    emit_event(simulator->uinput_fd, EV_ABS, ABS_MT_TRACKING_ID, -1);
    emit_event(simulator->uinput_fd, EV_SYN, SYN_REPORT, 0);
}
