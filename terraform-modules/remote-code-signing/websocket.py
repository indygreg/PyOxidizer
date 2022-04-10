# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Lambda function for processing websocket requests.

import json
import os
import time
import traceback

import boto3
import botocore


DYNAMODB_SESSIONS_TABLE = os.environ["DYNAMODB_SESSIONS_TABLE"]
CLOUDWATCH_NAMESPACE = os.environ["CLOUDWATCH_NAMESPACE"]
API_GATEWAY_MANAGEMENT_URL = os.environ["API_GATEWAY_MANAGEMENT_URL"]
MAX_SESSION_TTL_SECONDS = int(os.environ["MAX_SESSION_TTL_SECONDS"])


def main(event, context):
    event_type = event["requestContext"]["eventType"]

    try:
        if event_type == "CONNECT":
            return {"statusCode": 200, "body": None}
        elif event_type == "MESSAGE":
            return process_websocket_message(event)
        elif event_type == "DISCONNECT":
            return process_websocket_disconnect(event)
        else:
            return {
                "statusCode": 200,
                "body": json.dumps({"message": "unknown event: %s" % event_type}),
            }
    except Exception:
        traceback.print_exc()
        return make_error({}, "UNHANDLED_EXCEPTION", "server-side exception")


def record_metric(name, unit, value, dimensions=None):
    dimensions = dimensions or {}

    # We can't send a PutMetric value because CloudWatch metric APIs literally
    # manipulate entries in a time series database. So we instead emit the
    # embedded metric format event, which CloudWatch logs automatically turn
    # into metrics for us.
    # https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/CloudWatch_Embedded_Metric_Format_Specification.html
    payload = {
        "_aws": {
            "Timestamp": int(time.time() * 1000),
            "CloudWatchMetrics": [
                {
                    "Namespace": CLOUDWATCH_NAMESPACE,
                    "Dimensions": [list(dimensions.keys())],
                    "Metrics": [{"Name": name, "Unit": unit}],
                }
            ],
        }
    }

    for k, v in dimensions.items():
        payload[k] = v

    payload[name] = value

    print(json.dumps(payload))


def increment_counter(name, dimensions=None):
    record_metric(name, "Count", 1, dimensions)


def make_server_message(request, message_type, payload=None, ttl=None):
    message = {
        "request_id": request.get("request_id"),
        "type": message_type,
    }

    if payload is not None:
        message["payload"] = payload

    if ttl is not None:
        message["ttl"] = ttl

    return message


def make_response(request, message_type, payload=None, ttl=None):
    message = make_server_message(request, message_type, payload=payload, ttl=ttl)

    body = json.dumps(message)

    record_metric(
        "response_body_size",
        "Bytes",
        len(body),
        dimensions={"message_type": message_type},
    )

    return {"statusCode": 200, "body": body}


def make_error(request, code, message):
    return make_response(request, "error", {"code": code, "message": message})


def send_message_to_connection(
    connection_id, request, message_type, payload=None, ttl=None
):
    """Send a message to a peer.

    `data` is the value to JSON encode and send to the peer.
    """
    message = make_server_message(request, message_type, payload=payload, ttl=ttl)

    client = boto3.client(
        "apigatewaymanagementapi", endpoint_url=API_GATEWAY_MANAGEMENT_URL
    )

    data = json.dumps(message)

    record_metric(
        "send_message_size", "Bytes", len(data), {"message_type": message_type}
    )

    client.post_to_connection(
        ConnectionId=connection_id,
        Data=data,
    )


def get_sessions_table():
    dynamodb = boto3.resource("dynamodb")

    return dynamodb.Table(DYNAMODB_SESSIONS_TABLE)


def get_session_info(event, request):
    """Obtain metadata about the session."""
    connection_id = event["requestContext"]["connectionId"]
    session_id = request["payload"]["session_id"]

    res = get_sessions_table().get_item(
        Key={"session_id": session_id},
        AttributesToGet=["a_connection_id", "b_connection_id", "ttl"],
    )

    a_id = res["Item"]["a_connection_id"]
    b_id = res["Item"]["b_connection_id"]

    if a_id == connection_id:
        role = "a"
        peer_id = b_id
    elif b_id == connection_id:
        role = "b"
        peer_id = a_id
    else:
        role = None
        peer_id = None

    return {
        "role": role,
        "a_id": a_id,
        "b_id": b_id,
        "peer_id": peer_id,
        "ttl": int(res["Item"]["ttl"]),
    }


def process_websocket_disconnect(event):
    """Handles a disconnect event for a websocket.

    We attempt to find any session associated with the socket and to send a
    message to the peer socket, notifying it if the disconnect.
    """
    connection_id = event["requestContext"]["connectionId"]

    table = get_sessions_table()

    res = table.scan(
        ProjectionExpression="session_id, a_connection_id, b_connection_id",
        FilterExpression="a_connection_id = :conn OR b_connection_id = :conn",
        ExpressionAttributeValues={":conn": connection_id},
    )

    for entry in res["Items"]:
        if entry["a_connection_id"] == connection_id:
            other_connection = entry["b_connection_id"]
        else:
            other_connection = entry["a_connection_id"]

        if other_connection:
            try:
                send_message_to_connection(
                    other_connection,
                    {},
                    "session-closed",
                    {
                        "reason": "peer disconnected",
                    },
                )
            except Exception:
                increment_counter("on_disconnect_send_peer_close_failed")
                pass

        try:
            table.delete_item(Key={"session_id": entry["session_id"]})
        except botocore.client.ClientError:
            increment_counter("on_disconnect_delete_session_failed")

    return {"statusCode": 200, "body": None}


def process_websocket_message(event):
    """Process a MESSAGE websocket event."""
    try:
        request = json.loads(event["body"])
    except Exception:
        return make_error(
            {}, "REQUEST_JSON_PARSE", "failed to parse JSON in message body"
        )

    api = request.get("api")

    dimensions = {"api": api}

    increment_counter("api_call", dimensions)
    record_metric("request_body_size", "Bytes", len(event["body"]), dimensions)

    if "request_id" not in request:
        return make_error(
            request, "NO_REQUEST_ID", "request is missing request_id field"
        )

    if api:
        fn = API_HANDLERS.get(api)
        if fn:
            return fn(event, request)
        else:
            make_error(request, "UNKNOWN_API", "unrecognized API method: %s" % api)
    else:
        return make_error(request, "NO_API", "no API method specified")


def handle_hello(event, request):
    return make_response(
        request,
        "greeting",
        {
            "apis": sorted(API_HANDLERS.keys()),
            "motd": None,
        },
    )


def handle_create_session(event, request):
    connection_id = event["requestContext"]["connectionId"]
    payload = request["payload"]
    session_id = payload["session_id"]
    ttl = int(payload.get("ttl"))
    context = payload.get("context")

    table = get_sessions_table()

    # Impose our global TTL limit to prevent server abuse.
    ttl = min(ttl, MAX_SESSION_TTL_SECONDS)

    attributes = {
        "session_id": session_id,
        "a_connection_id": connection_id,
        "b_connection_id": None,
        "a_context": context,
        "ttl": int(time.time()) + ttl,
    }

    table.put_item(Item=attributes)

    return make_response(request, "session-created", None, ttl=ttl)


def handle_join_session(event, request):
    connection_id = event["requestContext"]["connectionId"]
    payload = request["payload"]
    session_id = payload["session_id"]
    context = payload.get("context")

    try:
        res = get_sessions_table().update_item(
            Key={"session_id": session_id},
            UpdateExpression="set b_connection_id = :conn_id",
            ExpressionAttributeValues={
                ":conn_id": connection_id,
            },
            # Don't allow multiple joins to the same session.
            ConditionExpression=boto3.dynamodb.conditions.Attr("b_connection_id").eq(
                None
            ),
            ReturnValues="ALL_NEW",
        )
    except botocore.client.ClientError:
        return make_error(
            request,
            "SESSION_JOIN_FAILED",
            "failed to join session; invalid or expired session id?",
        )

    ttl = int(res["Attributes"]["ttl"]) - int(time.time())

    send_message_to_connection(
        res["Attributes"]["a_connection_id"],
        request,
        "session-joined",
        {
            "context": context,
        },
        ttl,
    )

    return make_response(
        request,
        "session-joined",
        {
            "context": res["Attributes"]["a_context"],
        },
        ttl,
    )


def handle_send_message(event, request):
    payload = request["payload"]
    message = payload["message"]

    info = get_session_info(event, request)

    # Prevent connections that aren't in this session from sending messages.
    # This mitigates a DoS vector.
    if info["role"] is None:
        return make_error(
            request,
            "NOT_SESSION_MEMBER",
            "cannot send messages to a session you don't belong to",
        )

    try:
        send_message_to_connection(
            info["peer_id"],
            request,
            "peer-message",
            {
                "message": message,
            },
            info["ttl"],
        )
    except Exception:
        return make_error(
            request,
            "MESSAGE_SEND_FAILURE",
            "error sending message to peer (perhaps it disconnected?)",
        )

    return make_response(request, "message-sent", info["ttl"])


def handle_goodbye(event, request):
    payload = request["payload"]
    reason = payload.get("reason")

    info = get_session_info(event, request)

    # Prevent connections that aren't in this session from sending messages.
    # This mitigates a DoS vector.
    if info["role"] is None:
        return make_error(
            request,
            "NOT_SESSION_MEMBER",
            "cannot close sessions you are not a member of",
        )

    try:
        send_message_to_connection(
            info["peer_id"], request, "session-closed", {"reason": reason}
        )
    except Exception:
        increment_counter("goodbye_peer_delivery_failed")

    return make_response(request, "session-closed")


API_HANDLERS = {
    "create-session": handle_create_session,
    "hello": handle_hello,
    "goodbye": handle_goodbye,
    "join-session": handle_join_session,
    "send-message": handle_send_message,
}
