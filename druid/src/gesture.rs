// Copyright 2019 The Druid Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::time::Duration;
use std::collections::{VecDeque, HashMap};

use crate::kurbo::Point;
use crate::Modifiers;

use crate::widget::{Widget, Controller};
use crate::{Env, Event, EventCtx, TimerToken, MouseButton, MouseButtons, MouseEvent, PointerEvent, PointerId, PointerType, Vec2};

#[derive(Debug, Clone, PartialEq)]
struct TwoFingersGesture {
    fingerOneId: PointerId,
    fingerTwoId: PointerId,

    fingerOnePos: Point,
    fingerTwoPos: Point,

    fingerOnePosCur: Point,
    fingerTwoPosCur: Point,

    zoom: f64,
}

impl TwoFingersGesture {
    fn center(&self) -> Point {
        self.fingerOnePosCur.midpoint(self.fingerTwoPosCur)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum GestureControllerState {
    Idle,
    OneFingerIdle,
    OneFingerPressed,
    OneFingerTap,
    TwoFingersIdle(TwoFingersGesture),
    TwoFingersPan,
    PinchPanGesture(TwoFingersGesture),
}

const TAP_DELAY: Duration = Duration::from_millis(50);
const TWOFINGERS_MIN_PINCH_TRESHOLD: f64 = 20f64;
const PINCH_ZOOM_GAIN: f64 = 1f64;

//const ZOOM_DELTA_MAX_TRESHOLD: f64 = 0.001;

pub struct GestureController {
    timer_token: TimerToken,
    state: GestureControllerState,
    pointers_track: HashMap<PointerId, VecDeque<Event>>,
}

fn pointer_event_unchecked(evt: &Event) -> &PointerEvent {
    match evt {
        Event::PointerDown(pointer_event)
        | Event::PointerUp(pointer_event)
        | Event::PointerMove(pointer_event)
        | Event::PointerEnter(pointer_event)
        | Event::PointerLeave(pointer_event) => {
            pointer_event
        }
        _ => {
            panic!("Event is not a PointerEvent");
        }
    }
}

fn compute_zoom_level(fingerOnePos: Point, fingerTwoPos: Point, gesture_state: &TwoFingersGesture) -> f64 {
    let initial_distance = gesture_state.fingerOnePos.distance(gesture_state.fingerTwoPos);
    let current_distance = fingerOnePos.distance(fingerTwoPos);

    (current_distance / initial_distance)  * PINCH_ZOOM_GAIN
}

impl GestureController {
    pub fn new() -> Self {
        GestureController {
            timer_token: TimerToken::INVALID,
            state: GestureControllerState::Idle,
            pointers_track: HashMap::new(),
        }
    }

    fn get_current_twofinger_gesture(&self) -> TwoFingersGesture {
        let events: Vec<(&PointerId, &VecDeque<Event>)> = self.pointers_track.iter().collect();
        let fingerOnePos = pointer_event_unchecked(events[0].1.back().unwrap()).pos;
        let fingerTwoPos = pointer_event_unchecked(events[1].1.back().unwrap()).pos;
        TwoFingersGesture {
            fingerOneId: events[0].0.clone(),
            fingerTwoId: events[1].0.clone(),

            fingerOnePos: fingerOnePos,
            fingerTwoPos: fingerTwoPos,

            fingerOnePosCur: fingerOnePos,
            fingerTwoPosCur: fingerTwoPos,

            zoom: 1.0,
        }
    }

    fn pointer_pos(&self, id: &PointerId) -> Option<Point> {
        if let Some(queue) = self.pointers_track.get(id) {
            let pos = pointer_event_unchecked(queue.back().unwrap()).pos;
            Some(pos)
        } else {
            None
        }
    }
}

impl<T, W: Widget<T>> Controller<T, W> for GestureController {
    fn event(&mut self, child: &mut W, ctx: &mut EventCtx, event: &Event, data: &mut T, env: &Env) {
        let mut pointers_changed = false;
        let process_event = match &event {
            Event::PointerDown(pointer_event) => {
                if let Some(queue) = self.pointers_track.get_mut(&pointer_event.id) {
                    queue.push_back(event.clone());
                } else {
                    pointers_changed = true;
                    let mut queue = VecDeque::new();
                    queue.push_back(event.clone());
                    self.pointers_track.insert(pointer_event.id.clone(), queue);
                }
                true
            },
            Event::PointerMove(pointer_event) => {
                if let Some(queue) = self.pointers_track.get_mut(&pointer_event.id) {
                    queue.push_back(event.clone());
                }
                // discard eventual PointerMove with no previous PointerDown
                true
            },
            Event::PointerUp(pointer_event) | Event::PointerLeave(pointer_event) => {
                self.pointers_track.remove(&pointer_event.id);
                pointers_changed = true;
                true
            }
            _ => {
                false
            }
        };
        if !process_event {
            child.event(ctx, event, data, env);
            return;
        }

        let new_state = match &self.state {
            GestureControllerState::Idle => {
                if self.pointers_track.len() == 2 {
                    GestureControllerState::TwoFingersIdle(
                        self.get_current_twofinger_gesture()
                    )                    
                } else {
                    self.state.clone()
                }
            },
            GestureControllerState::TwoFingersIdle(gesture_state) => {
                if pointers_changed {
                    GestureControllerState::Idle
                } else {
                    let fingerOneCurrentPos = self.pointer_pos(&gesture_state.fingerOneId);
                    let fingerTwoCurrentPos = self.pointer_pos(&gesture_state.fingerTwoId);
                    let fingerOneDistance = 
                        gesture_state.fingerOnePos.distance(fingerOneCurrentPos.unwrap());
                    let fingerTwoDistance =
                        gesture_state.fingerTwoPos.distance(fingerTwoCurrentPos.unwrap());
                    if fingerOneDistance.abs() > TWOFINGERS_MIN_PINCH_TRESHOLD ||
                       fingerTwoDistance.abs() > TWOFINGERS_MIN_PINCH_TRESHOLD {  
                        GestureControllerState::PinchPanGesture(gesture_state.clone())
                    } else {
                        self.state.clone()
                    }
                }
            },
            GestureControllerState::PinchPanGesture(gesture_state) => {
                if pointers_changed {
                    GestureControllerState::Idle
                } else {
                    let fingerOneCurrentPos = self.pointer_pos(&gesture_state.fingerOneId);
                    let fingerTwoCurrentPos = self.pointer_pos(&gesture_state.fingerTwoId);

                    let mut new_state = gesture_state.clone();
                    new_state.zoom = compute_zoom_level(
                        fingerOneCurrentPos.unwrap(),
                        fingerTwoCurrentPos.unwrap(),
                        &gesture_state);
                    new_state.fingerOnePosCur = fingerOneCurrentPos.unwrap();
                    new_state.fingerTwoPosCur = fingerTwoCurrentPos.unwrap();
                    GestureControllerState::PinchPanGesture(new_state)
                }
            },
            _ => self.state.clone()
        };

        match (&self.state, &new_state) {
            (GestureControllerState::PinchPanGesture(previous_state),
             GestureControllerState::PinchPanGesture(gesture_state)) => {
                 let zoom_event = Event::GestureZoom {
                     zoom: gesture_state.zoom - previous_state.zoom,
                     center: gesture_state.center(),
                 };
                 let pan_event = Event::GesturePan(
                     previous_state.center().to_vec2() -  gesture_state.center().to_vec2()
                 );
                 //child.event(ctx, &pan_event, data, env);
                 child.event(ctx, &zoom_event, data, env);
            },
            _ => {}
        }

        if self.state != new_state {
            //log::debug!("New Recognizer State: {:?}", new_state);
        }
        self.state = new_state;
    }
}