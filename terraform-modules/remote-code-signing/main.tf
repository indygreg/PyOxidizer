variable "lambda_zip_path" {
  description = "Path to write Lambda function zip file to"
  type        = string
}

variable "lambda_function_name" {
  description = "Name of Lambda function to create"
  type        = string
  default     = "remote-code-sign-websocket"
}

variable "dynamodb_table_name" {
  description = "Name of DynamoDB table to store state in"
  type        = string
  default     = "remote-code-signing-sessions"
}

variable "api_gateway_api_name" {
  description = "Name to give to API Gateway API"
  type        = string
  default     = "remote-code-signing-websocket"
}

variable "api_gateway_role_name" {
  description = "Name of IAM role to associate with API Gateway (to allow CloudWatch logging)"
  type        = string
  default     = "apigateway-cloudwatch"
}

variable "lambda_role_name" {
  description = "Name of IAM role to associate with Lambda function invocations"
  type        = string
  default     = "websocket-lambda"
}

variable "api_gateway_invoke_role_name" {
  description = "Name of IAM role to use in API Gateway for invoking Lambda"
  type        = string
  default     = "apigateway-lambda-invoke"
}

variable "cloudwatch_namespace" {
  description = "CloudWatch namespace to use for metrics collection"
  type        = string
  default     = "RemoteCodeSigning"
}

variable "hostname" {
  description = "Hostname to use for API Gateway"
  type        = string
  default     = null
}

variable "lambda_cloudwatch_logs_retention_days" {
  description = "Number of days of CloudWatch logs to retain for Lambda function"
  type        = number
  default     = 3
}

variable "api_gateway_cloudwatch_logs_retention_days" {
  description = "Number of days of CloudWatch logs to retain for API Gateway"
  type        = number
  default     = 3
}

variable "max_session_ttl_seconds" {
  description = "Maximum signing session duration in seconds"
  type        = number
  default     = 1800
}

variable "dashboard_name" {
  description = "Name of CloudWatch Dashboard"
  type        = string
  default     = "RemoteCodeSigning"
  nullable    = true
}

locals {
  have_hostname              = var.hostname != null
  stage_name                 = "main"
  api_gateway_management_url = "https://${aws_apigatewayv2_api.websocket.id}.execute-api.${data.aws_region.current.name}.amazonaws.com/${local.stage_name}"
}

data "aws_region" "current" {}

data "archive_file" "lambda" {
  type        = "zip"
  source_file = "${path.module}/websocket.py"
  output_path = var.lambda_zip_path
}

data "aws_iam_policy_document" "assume_role_apigateway" {
  statement {
    effect = "Allow"
    principals {
      type        = "Service"
      identifiers = ["apigateway.amazonaws.com"]
    }
    actions = ["sts:AssumeRole"]
  }
}

data "aws_iam_policy_document" "assume_role_lambda" {
  statement {
    effect = "Allow"
    principals {
      type        = "Service"
      identifiers = ["lambda.amazonaws.com"]
    }
    actions = ["sts:AssumeRole"]
  }
}

resource "aws_iam_role" "api_gateway" {
  name               = var.api_gateway_role_name
  description        = "Used to allow API Gateway to log to CloudWatch"
  assume_role_policy = data.aws_iam_policy_document.assume_role_apigateway.json
}

resource "aws_iam_role" "api_gateway_invoke" {
  name               = var.api_gateway_invoke_role_name
  description        = "Used to allow API Gateway to invoke Lambda functions"
  assume_role_policy = data.aws_iam_policy_document.assume_role_apigateway.json
}

resource "aws_iam_role" "lambda" {
  name               = var.lambda_role_name
  description        = "Used to invoke Lambda functions handling websockets"
  assume_role_policy = data.aws_iam_policy_document.assume_role_lambda.json
}

resource "aws_api_gateway_account" "account" {
  cloudwatch_role_arn = aws_iam_role.api_gateway.arn
}

resource "aws_apigatewayv2_api" "websocket" {
  name                       = var.api_gateway_api_name
  description                = "Forwards websocket requests for remote code signing"
  protocol_type              = "WEBSOCKET"
  route_selection_expression = "$request.body.action"
}

resource "aws_dynamodb_table" "sessions" {
  name         = var.dynamodb_table_name
  billing_mode = "PAY_PER_REQUEST"

  attribute {
    name = "session_id"
    type = "S"
  }

  hash_key = "session_id"

  ttl {
    enabled        = true
    attribute_name = "ttl"
  }
}

resource "aws_lambda_function" "websocket" {
  function_name    = var.lambda_function_name
  description      = "Handles websocket connections"
  role             = aws_iam_role.lambda.arn
  filename         = data.archive_file.lambda.output_path
  source_code_hash = data.archive_file.lambda.output_base64sha256
  runtime          = "python3.9"
  handler          = "websocket.main"
  timeout          = 30

  environment {
    variables = {
      API_GATEWAY_MANAGEMENT_URL = local.api_gateway_management_url
      DYNAMODB_SESSIONS_TABLE    = aws_dynamodb_table.sessions.name
      CLOUDWATCH_NAMESPACE       = var.cloudwatch_namespace
      MAX_SESSION_TTL_SECONDS    = var.max_session_ttl_seconds
    }
  }
}

# Enable API Gateway to invoke Lambda functions.
resource "aws_lambda_permission" "api_gateway_connect" {
  statement_id  = "AllowExecutionFromAPIGateway"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.websocket.function_name
  principal     = "apigateway.amazonaws.com"
  source_arn    = "${aws_apigatewayv2_api.websocket.execution_arn}/*/*"
}

resource "aws_apigatewayv2_integration" "websocket" {
  api_id                    = aws_apigatewayv2_api.websocket.id
  integration_type          = "AWS_PROXY"
  integration_method        = "POST"
  integration_uri           = aws_lambda_function.websocket.invoke_arn
  passthrough_behavior      = "WHEN_NO_MATCH"
  content_handling_strategy = "CONVERT_TO_TEXT"
  credentials_arn           = aws_iam_role.api_gateway_invoke.arn
}

resource "aws_apigatewayv2_route" "connect" {
  api_id             = aws_apigatewayv2_api.websocket.id
  route_key          = "$connect"
  authorization_type = "NONE"
  target             = "integrations/${aws_apigatewayv2_integration.websocket.id}"
}

resource "aws_apigatewayv2_route" "default" {
  api_id             = aws_apigatewayv2_api.websocket.id
  route_key          = "$default"
  authorization_type = "NONE"
  target             = "integrations/${aws_apigatewayv2_integration.websocket.id}"
}

resource "aws_apigatewayv2_route" "disconnect" {
  api_id             = aws_apigatewayv2_api.websocket.id
  route_key          = "$disconnect"
  authorization_type = "NONE"
  target             = "integrations/${aws_apigatewayv2_integration.websocket.id}"
}

resource "aws_apigatewayv2_route_response" "default" {
  api_id             = aws_apigatewayv2_api.websocket.id
  route_id           = aws_apigatewayv2_route.default.id
  route_response_key = "$default"
}

resource "aws_apigatewayv2_deployment" "websocket" {
  description = "Deployment for websocket API Gateway"
  api_id      = aws_apigatewayv2_api.websocket.id

  lifecycle {
    create_before_destroy = true
  }

  triggers = {
    redeployment = sha1(jsonencode([
      aws_apigatewayv2_integration.websocket,

      aws_apigatewayv2_route.connect,
      aws_apigatewayv2_route.default,
      aws_apigatewayv2_route.disconnect,
    ]))
  }
}

resource "aws_apigatewayv2_stage" "stage" {
  api_id        = aws_apigatewayv2_api.websocket.id
  name          = local.stage_name
  deployment_id = aws_apigatewayv2_deployment.websocket.id
  access_log_settings {
    destination_arn = aws_cloudwatch_log_group.api_gateway.arn
    format          = "$context.identity.caller $context.identity.user \"$context.eventType $context.routeKey $context.connectionId\" $context.status  $context.requestId"
  }
  default_route_settings {
    data_trace_enabled     = false
    logging_level          = "ERROR"
    throttling_burst_limit = 100
    throttling_rate_limit  = 100
  }
}

resource "aws_acm_certificate" "websocket" {
  count             = local.have_hostname ? 1 : 0
  domain_name       = var.hostname
  validation_method = "DNS"
  options {
    certificate_transparency_logging_preference = "ENABLED"
  }
}

output "domain_validation_options" {
  value = local.have_hostname ? aws_acm_certificate.websocket[0].domain_validation_options : null
}

resource "aws_apigatewayv2_domain_name" "websocket" {
  count       = local.have_hostname ? 1 : 0
  domain_name = var.hostname

  domain_name_configuration {
    certificate_arn = aws_acm_certificate.websocket[0].arn
    endpoint_type   = "REGIONAL"
    security_policy = "TLS_1_2"
  }
}

output "domain_name" {
  value = local.have_hostname ? aws_apigatewayv2_domain_name.websocket[0].domain_name_configuration[0].target_domain_name : null
}

output "domain_zone_id" {
  value = local.have_hostname ? aws_apigatewayv2_domain_name.websocket[0].domain_name_configuration[0].hosted_zone_id : null
}

resource "aws_apigatewayv2_api_mapping" "websocket" {
  count       = local.have_hostname ? 1 : 0
  api_id      = aws_apigatewayv2_api.websocket.id
  domain_name = aws_apigatewayv2_domain_name.websocket[0].id
  stage       = aws_apigatewayv2_stage.stage.id
}

resource "aws_cloudwatch_log_group" "lambda" {
  name              = "/aws/lambda/${aws_lambda_function.websocket.function_name}"
  retention_in_days = var.lambda_cloudwatch_logs_retention_days
}

resource "aws_cloudwatch_log_metric_filter" "lambda_python_exceptions" {
  name           = "Python Exceptions"
  log_group_name = aws_cloudwatch_log_group.lambda.name
  pattern        = "\"Traceback (most recent call last):\""
  metric_transformation {
    namespace = var.cloudwatch_namespace
    name      = "exceptions"
    value     = "1"
    unit      = "Count"
  }
}

resource "aws_cloudwatch_log_group" "api_gateway" {
  name              = "/aws/apigateway/${aws_apigatewayv2_api.websocket.id}/${local.stage_name}"
  retention_in_days = var.api_gateway_cloudwatch_logs_retention_days
}

data "aws_iam_policy_document" "lambda_role" {
  # Allow Lambda function to write CloudWatch events.
  statement {
    effect = "Allow"
    actions = [
      "logs:CreateLogGroup",
      "logs:CreateLogStream",
      "logs:PutLogEvents",
    ]
    resources = [
      aws_cloudwatch_log_group.lambda.arn,
      "${aws_cloudwatch_log_group.lambda.arn}:*",
    ]
  }

  # Allow Lambda function to read-write to DynamoDB.
  statement {
    effect = "Allow"
    actions = [
      "dynamodb:DeleteItem",
      "dynamodb:GetItem",
      "dynamodb:PutItem",
      "dynamodb:Scan",
      "dynamodb:UpdateItem",
    ]
    resources = [
      aws_dynamodb_table.sessions.arn,
    ]
  }

  # Allow Lambda function to write to websocket connections.
  statement {
    effect = "Allow"
    actions = [
      "execute-api:Invoke",
      "execute-api:ManageConnections",
    ]
    resources = [
      "${aws_apigatewayv2_stage.stage.execution_arn}/*",
    ]
  }
}

resource "aws_iam_role_policy" "lambda" {
  role   = aws_iam_role.lambda.name
  name   = aws_iam_role.lambda.name
  policy = data.aws_iam_policy_document.lambda_role.json
}

resource "aws_iam_role_policy_attachment" "api_gateway" {
  role       = aws_iam_role.api_gateway.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonAPIGatewayPushToCloudWatchLogs"
}

data "aws_iam_policy_document" "api_gateway_invoke" {
  statement {
    effect = "Allow"
    actions = [
      "lambda:InvokeFunction"
    ]
    resources = ["*"]
  }
}

resource "aws_iam_role_policy" "api_gateway_invoke" {
  role   = aws_iam_role.api_gateway_invoke.name
  name   = "APIGatewayInvokeLambdaFunction"
  policy = data.aws_iam_policy_document.api_gateway_invoke.json
}

resource "aws_cloudwatch_dashboard" "dashboard" {
  count          = var.dashboard_name != null ? 1 : 0
  dashboard_name = var.dashboard_name
  dashboard_body = jsonencode(
    {
      widgets = [
        {
          height = 6
          properties = {
            legend = {
              position = "bottom"
            }
            metrics = [
              [
                {
                  expression = "SELECT COUNT(api_call) FROM ${var.cloudwatch_namespace} GROUP BY api"
                  id         = "api_counts"
                  label      = ""
                  region     = data.aws_region.current.name
                },
              ],
            ]
            period               = 300
            region               = data.aws_region.current.name
            setPeriodToTimeRange = true
            stacked              = true
            stat                 = "Average"
            title                = "API Calls"
            view                 = "timeSeries"
            yAxis = {
              left = {
                showUnits = false
              }
            }
          }
          type  = "metric"
          width = 6
          x     = 0
          y     = 0
        },
        {
          height = 6
          properties = {
            legend = {
              position = "hidden"
            }
            metrics = [
              [
                {
                  expression = "SELECT SUM(request_body_size) FROM ${var.cloudwatch_namespace}"
                  id         = "request_sizes"
                  label      = ""
                  region     = data.aws_region.current.name
                },
              ],
            ]
            period               = 300
            region               = data.aws_region.current.name
            setPeriodToTimeRange = true
            stacked              = true
            stat                 = "Average"
            title                = "Total Request Size (Bytes)"
            view                 = "timeSeries"
            yAxis = {
              left = {
                showUnits = true
              }
            }
          }
          type  = "metric"
          width = 6
          x     = 6
          y     = 0
        },
        {
          height = 6
          properties = {
            legend = {
              position = "hidden"
            }
            metrics = [
              [
                {
                  expression = "SELECT SUM(response_body_size) FROM ${var.cloudwatch_namespace}"
                  id         = "response_sizes"
                  label      = ""
                  region     = data.aws_region.current.name
                },
              ],
            ]
            period               = 300
            region               = data.aws_region.current.name
            setPeriodToTimeRange = true
            stacked              = true
            stat                 = "Average"
            title                = "Total Response Size (Bytes)"
            view                 = "timeSeries"
            yAxis = {
              left = {
                showUnits = true
              }
            }
          }
          type  = "metric"
          width = 6
          x     = 12
          y     = 0
        },
        {
          height = 6
          properties = {
            metrics = [
              [
                "AWS/DynamoDB",
                "ConsumedReadCapacityUnits",
                "TableName",
                "remote-code-signing-sessions",
              ],
              [
                ".",
                "ConsumedWriteCapacityUnits",
                ".",
                ".",
              ],
            ]
            region  = data.aws_region.current.name
            stacked = false
            title   = "DynamoDB Usage"
            view    = "timeSeries"
          }
          type  = "metric"
          width = 6
          x     = 18
          y     = 0
        },
        {
          height = 6
          properties = {
            legend = {
              position = "hidden"
            }
            metrics = [[
              {
                expression = "SELECT COUNT(exceptions) FROM ${var.cloudwatch_namespace}"
                id         = "q1"
                label      = ""
                yAxis      = "left"
              }
            ]]

            period               = 300
            region               = data.aws_region.current.name
            setPeriodToTimeRange = true
            stacked              = true
            stat                 = "Average"
            title                = "Python Exceptions"
            view                 = "timeSeries"
            yAxis = {
              left = {
                showUnits = false
              }
          } }
          type = "metric"

          width = 6
          x     = 0
          y     = 6
        }
      ]

    }
  )
}