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

//! Druid implementation of gesture recognition

//use std::time::Duration;
use std::collections::{VecDeque, HashMap};

use crate::kurbo::Point;

use crate::{Event, PointerEvent, PointerId};

pub trait GestureRecognizer {
    fn process_event(&mut self, event: &Event) -> VecDeque<Event>;
}

#[derive(Debug, Clone, PartialEq)]
struct TwoFingersGesture {
    finger_one_id: PointerId,
    finger_two_id: PointerId,

    finger_one_pos: Point,
    finger_two_pos: Point,

    finger_one_pos_cur: Point,
    finger_two_pos_cur: Point,

    zoom: f64,
}

impl TwoFingersGesture {
    fn center(&self) -> Point {
        self.finger_one_pos_cur.midpoint(self.finger_two_pos_cur)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum GestureRecognizerState {
    Idle,
    //OneFingerIdle,
    //OneFingerPressed,
    //OneFingerTap,
    TwoFingersIdle(TwoFingersGesture),
    PinchPanGesture(TwoFingersGesture),
}

//const TAP_DELAY: Duration = Duration::from_millis(50);
const TWOFINGERS_MIN_PINCH_TRESHOLD: f64 = 20f64;
const PINCH_ZOOM_GAIN: f64 = 1f64;

//const ZOOM_DELTA_MAX_TRESHOLD: f64 = 0.001;

/// Implements the state machine for recognizing gestures
pub struct DruidGestureRecognizer {
    state: GestureRecognizerState,
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

fn compute_zoom_level(finger_one_pos: Point, finger_two_pos: Point, gesture_state: &TwoFingersGesture) -> f64 {
    let initial_distance = gesture_state.finger_one_pos.distance(gesture_state.finger_two_pos);
    let current_distance = finger_one_pos.distance(finger_two_pos);

    (current_distance / initial_distance)  * PINCH_ZOOM_GAIN
}

impl DruidGestureRecognizer {
    /// Creates a new gesture recognition state machine
    pub fn new() -> Self {
        DruidGestureRecognizer {
            state: GestureRecognizerState::Idle,
            pointers_track: HashMap::new(),
        }
    }

    fn get_current_twofinger_gesture(&self) -> TwoFingersGesture {
        let events: Vec<(&PointerId, &VecDeque<Event>)> = self.pointers_track.iter().collect();
        let finger_one_pos = pointer_event_unchecked(events[0].1.back().unwrap()).pos;
        let finger_two_pos = pointer_event_unchecked(events[1].1.back().unwrap()).pos;
        TwoFingersGesture {
            finger_one_id: events[0].0.clone(),
            finger_two_id: events[1].0.clone(),

            finger_one_pos: finger_one_pos,
            finger_two_pos: finger_two_pos,

            finger_one_pos_cur: finger_one_pos,
            finger_two_pos_cur: finger_two_pos,

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

    /// Returns true if pointers have been added or removed
    fn update_pointers(&mut self, event: &Event) -> bool {
        let mut pointers_changed = false;
        match event {
            Event::PointerDown(pointer_event) => {
                if let Some(queue) = self.pointers_track.get_mut(&pointer_event.id) {
                    queue.push_back(event.clone());
                } else {
                    pointers_changed = true;
                    let mut queue = VecDeque::new();
                    queue.push_back(event.clone());
                    self.pointers_track.insert(pointer_event.id.clone(), queue);
                }
            },
            Event::PointerMove(pointer_event) => {
                if let Some(queue) = self.pointers_track.get_mut(&pointer_event.id) {
                    queue.push_back(event.clone());
                }
                // discard eventual PointerMove with no previous PointerDown
            },
            Event::PointerUp(pointer_event) | Event::PointerLeave(pointer_event) => {
                self.pointers_track.remove(&pointer_event.id);
                pointers_changed = true;
            }
            _ => {}
        };

        pointers_changed
    }
}

impl GestureRecognizer for DruidGestureRecognizer {
    fn process_event(&mut self, event: &Event) -> VecDeque<Event> {
        let pointers_changed = self.update_pointers(&event);

        let new_state = match &self.state {
            GestureRecognizerState::Idle => {
                if self.pointers_track.len() == 2 {
                    GestureRecognizerState::TwoFingersIdle(
                        self.get_current_twofinger_gesture()
                    )                    
                } else {
                    self.state.clone()
                }
            },
            GestureRecognizerState::TwoFingersIdle(gesture_state) => {
                if pointers_changed {
                    GestureRecognizerState::Idle
                } else {
                    let finger_one_current_pos = self.pointer_pos(&gesture_state.finger_one_id);
                    let finger_two_current_pos = self.pointer_pos(&gesture_state.finger_two_id);
                    let finger_one_distance =
                        gesture_state.finger_one_pos.distance(finger_one_current_pos.unwrap());
                    let finger_two_distance =
                        gesture_state.finger_two_pos.distance(finger_two_current_pos.unwrap());
                    if finger_one_distance.abs() > TWOFINGERS_MIN_PINCH_TRESHOLD ||
                       finger_two_distance.abs() > TWOFINGERS_MIN_PINCH_TRESHOLD {  
                        GestureRecognizerState::PinchPanGesture(gesture_state.clone())
                    } else {
                        self.state.clone()
                    }
                }
            },
            GestureRecognizerState::PinchPanGesture(gesture_state) => {
                if pointers_changed {
                    GestureRecognizerState::Idle
                } else {
                    let finger_one_current_pos = self.pointer_pos(&gesture_state.finger_one_id);
                    let finger_two_current_pos = self.pointer_pos(&gesture_state.finger_two_id);

                    let mut new_state = gesture_state.clone();
                    new_state.zoom = compute_zoom_level(
                        finger_one_current_pos.unwrap(),
                        finger_two_current_pos.unwrap(),
                        &gesture_state);
                    new_state.finger_one_pos_cur = finger_one_current_pos.unwrap();
                    new_state.finger_two_pos_cur = finger_two_current_pos.unwrap();
                    GestureRecognizerState::PinchPanGesture(new_state)
                }
            },
        };

        let mut gesture_events = VecDeque::<Event>::new();
        match (&self.state, &new_state) {
            (GestureRecognizerState::PinchPanGesture(previous_state),
             GestureRecognizerState::PinchPanGesture(gesture_state)) => {
                 let zoom_event = Event::GestureZoom {
                     zoom: gesture_state.zoom - previous_state.zoom,
                     center: gesture_state.center(),
                 };
                 let pan_event = Event::GesturePan(
                     previous_state.center().to_vec2() -  gesture_state.center().to_vec2()
                 );

                 gesture_events.push_back(pan_event);
                 gesture_events.push_back(zoom_event);
            },
            _ => {}
        }

        if self.state != new_state {
            log::debug!("New Recognizer State: {:?}", new_state);
        }
        self.state = new_state;

        gesture_events
    }
}
