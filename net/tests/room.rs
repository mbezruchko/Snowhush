use std::time::Duration;

use snowhush_net::{Event, Room, host, join};

async fn wait_for(room: &Room, what: impl Fn(&Event) -> bool) -> Event {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(event) = room.next() {
            if what(&event) {
                return event;
            }
        }
        assert!(tokio::time::Instant::now() < deadline, "timed out");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn friends_meet_and_throw() {
    let hosting = host("Host".into(), None).await.unwrap();
    let Event::Room { ticket, me: host_id } = wait_for(&hosting, |e| matches!(e, Event::Room { .. })).await else { unreachable!() };
    println!("ticket is {} characters", ticket.len());

    let anna = join(format!("snowhush://join/{ticket}"), "Anna".into(), None).await.unwrap();
    let Event::Joined { me: anna_id } = wait_for(&anna, |e| matches!(e, Event::Joined { .. })).await else { unreachable!() };
    // Anna is told of the host, and the host of Anna.
    assert_eq!(wait_for(&anna, |e| matches!(e, Event::Player { .. })).await, Event::Player { id: host_id.clone(), name: "Host".into() });
    assert_eq!(wait_for(&hosting, |e| matches!(e, Event::Player { .. })).await, Event::Player { id: anna_id.clone(), name: "Anna".into() });

    let boris = join(ticket.clone(), "Boris".into(), None).await.unwrap();
    let Event::Joined { me: boris_id } = wait_for(&boris, |e| matches!(e, Event::Joined { .. })).await else { unreachable!() };
    // Boris hears of both; Anna of Boris.
    let mut heard = vec![];
    for _ in 0..2 {
        if let Event::Player { name, .. } = wait_for(&boris, |e| matches!(e, Event::Player { .. })).await {
            heard.push(name);
        }
    }
    heard.sort();
    assert_eq!(heard, vec!["Anna".to_string(), "Host".to_string()]);
    assert_eq!(wait_for(&anna, |e| matches!(e, Event::Player { .. })).await, Event::Player { id: boris_id.clone(), name: "Boris".into() });

    // Anna hits Boris: the host and Boris hear it.
    anna.throw(&boris_id);
    let expected = Event::Throw { from: anna_id.clone(), at: boris_id.clone() };
    assert_eq!(wait_for(&boris, |e| matches!(e, Event::Throw { .. })).await, expected);
    assert_eq!(wait_for(&hosting, |e| matches!(e, Event::Throw { .. })).await, expected);
    // The host hits Anna.
    hosting.throw(&anna_id);
    assert_eq!(wait_for(&anna, |e| matches!(e, Event::Throw { .. })).await, Event::Throw { from: host_id.clone(), at: anna_id.clone() });

    // Boris goes: the others hear it. Then the host goes: Anna's room is closed.
    boris.leave().await;
    assert_eq!(wait_for(&anna, |e| matches!(e, Event::Left { .. })).await, Event::Left { id: boris_id.clone() });
    hosting.leave().await;
    assert!(matches!(wait_for(&anna, |e| matches!(e, Event::Closed { .. })).await, Event::Closed { .. }));
}

/// Everything that comes within `time`.
async fn drain(room: &Room, time: Duration) -> Vec<Event> {
    let deadline = tokio::time::Instant::now() + time;
    let mut events = vec![];
    while tokio::time::Instant::now() < deadline {
        while let Some(event) = room.next() {
            events.push(event);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    events
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coming_back_leaves_no_ghost() {
    let hosting = host("Host".into(), None).await.unwrap();
    let Event::Room { ticket, .. } = wait_for(&hosting, |e| matches!(e, Event::Room { .. })).await else { unreachable!() };
    let anna = join(ticket.clone(), "Anna".into(), None).await.unwrap();
    let Event::Joined { me: anna_id } = wait_for(&anna, |e| matches!(e, Event::Joined { .. })).await else { unreachable!() };
    wait_for(&hosting, |e| matches!(e, Event::Player { .. })).await;

    // Anna comes back while the host still holds her way from before, as after her Mac slept: she is who she was.
    let again = join(ticket.clone(), "Anna".into(), None).await.unwrap();
    assert_eq!(wait_for(&again, |e| matches!(e, Event::Joined { .. })).await, Event::Joined { me: anna_id.clone() });
    let heard = drain(&again, Duration::from_secs(2)).await;
    assert!(!heard.iter().any(|e| matches!(e, Event::Player { id, .. } if *id == anna_id)), "{heard:?}");
    let seen = drain(&hosting, Duration::from_secs(1)).await;
    assert!(!seen.contains(&Event::Left { id: anna_id.clone() }), "{seen:?}");

    // Someone new hears of her once.
    let boris = join(ticket, "Boris".into(), None).await.unwrap();
    let players: Vec<Event> = drain(&boris, Duration::from_secs(3)).await.into_iter().filter(|e| matches!(e, Event::Player { .. })).collect();
    assert_eq!(players.len(), 2, "{players:?}");
    anna.leave().await;
    again.leave().await;
    boris.leave().await;
    hosting.leave().await;
}
