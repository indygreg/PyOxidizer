===============================================
Terraform Module for Remote Code Signing Server
===============================================

This directory defines a Terraform module which provides a websocket server
facilitating the exchange of messages between code signing peers.

Overview
========

The Terraform module defines AWS resources for deploying a websocket server
leveraging Lambda, API Gateway, and DynamoDB. To support this service it also
needs to provision IAM and CloudWatch resources. It currently also uses ACM
for an Amazon issued certificate to be used for API Gateway. However, this
requirement could possibly be made optional.

The ``websocket.py`` file is registered as a Lambda function. This Lambda
function handles websocket connect, disconnect, and message events.

Persistent state for the Lambda function is stored in a single DynamoDB table.
Items in the table have a TTL, ensuring they automatically expire and don't
accumulate space/costs.

An API Gateway with a single stage is configured to send all websocket activity
to the Lambda. There is no authentication for the API Gateway, so anyone with
network access can speak to it.

A custom API Gateway domain name is optionally registered and the API Gateway is
bound to it if the ``hostname`` variable is passed to the module.

Example Usage
=============

This is a minimal example. It will not configure a domain name for API
Gateway.

.. code-block:: terraform

    module "remote_code_signing" {
      source = "/path/to/remote-code-signing"
      lambda_zip_path = "${path.root}/lambda.zip"
    }

This example configures a hostname and registers Route 53 records for its
DNS resolution.

.. code-block:: terraform

    locals {
      route53_zone_id = "<insert your own value or resource reference>"
      hostname = "ws.codesign.gregoryszorc.com"
    }

    module "remote_code_signing" {
      source = "/path/to/remote-code-signing"
      lambda_zip_path = "${path.root}/lambda.zip"
      hostname = local.hostname
    }

    resource "aws_route53_record" "codesign" {
      name = local.hostname
      type = "A"
      zone_id = local.route53_zone_id
      alias {
        evaluate_target_health = true
        name = module.remote_code_signing.domain_name
        zone_id = module.remote_code_signing.domain_zone_id
      }
    }

    resource "aws_route53_record" "codesign_validation_validation" {
      for_each = {
        for dvo in module.remote_code_signing.domain_validation_options : dvo.resource_record_name => {
          name = dvo.resource_record_name
          record = dvo.resource_record_value
          type = dvo.resource_record_type
        }
      }
      name = each.value.name
      records = [each.value.record]
      type = each.value.type
      zone_id = local.route53_zone_id
      ttl = 60
      allow_overwrite = true
    }

Additional Customization
========================

The module has various additional variables to control behavior. e.g.
the names of most resources can be modified from their defaults. Look
for ``variable`` blocks at the top of ``main.tf`` for more.
